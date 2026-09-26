//! Fixed, version-specific vendor protocol. No arbitrary OIDs or command input.
use std::{thread, time::Duration};

pub type Result<T> = std::result::Result<T, String>;
pub const DRIVER_HASH: &str = "378ffaee3b782b9a7f5382c294a846e7c4b3b952717991fd1fd93795c5a85612";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Country,
    Info,
    ManualUs,
    Done,
    Output,
}

fn put(out: &mut Vec<u8>, value: u32) {
    out.extend(value.to_le_bytes());
}
fn word(b: &[u8], offset: usize) -> Result<u32> {
    let bytes = b.get(offset..offset + 4).ok_or("响应截断")?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}

pub fn packet(command: Command) -> Vec<u8> {
    let mut body = Vec::new();
    let oid = match command {
        Command::Country => 0xff81521f,
        Command::Info | Command::ManualUs => {
            for value in [0, 0, if command == Command::ManualUs { 29 } else { 0 }, 3] {
                put(&mut body, value);
            }
            let tokens: &[&[u8]] = if command == Command::ManualUs {
                &[b"67", b"US"]
            } else {
                &[b"echo", b"core", b"6g_info"]
            };
            for token in tokens {
                let end = body.len() + 16;
                body.extend(*token);
                body.resize(end, 0);
            }
            body.resize(336, 0);
            0xff818040
        }
        Command::Done => 0xff818041,
        Command::Output => 0xff818042,
    };
    if body.is_empty() {
        body.resize(300, 0);
    }
    let n = body.len() as u32;
    let mut result = Vec::new();
    for value in [n + 36, 0x814ce000, 4, 1, 7, n + 40, oid, n + 24, n + 24, n] {
        put(&mut result, value);
    }
    result.extend(body);
    result
}

pub fn decode<'a>(request: &[u8], response: &'a [u8], returned: usize) -> Result<&'a [u8]> {
    if response.len() != request.len() || !(40..=response.len()).contains(&returned) {
        return Err("响应长度无效".into());
    }
    if request[..28] != response[..28] || request[36..40] != response[36..40] {
        return Err("响应协议头不匹配".into());
    }
    let written = word(response, 28)? as usize;
    if word(response, 32)? != 0 || written > word(response, 36)? as usize || 40 + written > returned
    {
        return Err("驱动未确认有效响应长度".into());
    }
    Ok(&response[40..40 + written])
}

pub trait Transport {
    fn exchange(&mut self, command: Command) -> Result<Vec<u8>>;
}

pub fn country(t: &mut impl Transport) -> Result<String> {
    let b = t.exchange(Command::Country)?;
    if b.len() != 2 || !b.iter().all(|c| (32..=126).contains(c)) {
        return Err("国家码响应无效".into());
    }
    Ok(String::from_utf8(b).unwrap())
}

fn done(t: &mut impl Transport) -> Result<bool> {
    let b = t.exchange(Command::Done)?;
    if b.len() != 4 || word(&b, 0)? > 1 {
        return Err("完成标志无效".into());
    }
    Ok(word(&b, 0)? == 1)
}

#[derive(Default)]
struct Output {
    sequence: Option<u32>,
    bytes: Vec<u8>,
}
impl Output {
    fn push(&mut self, b: &[u8]) -> Result<bool> {
        let (mode, status, sequence, len) =
            (word(b, 0)?, word(b, 4)?, word(b, 8)?, word(b, 12)? as usize);
        if mode != 0 || ![0, 2].contains(&status) || len > 200 || b.len() != 16 + len {
            return Err("输出块格式无效".into());
        }
        if self.sequence.is_some_and(|s| s != sequence) {
            return Err("输出序列改变：可能有其他诊断程序并发使用".into());
        }
        self.sequence = Some(sequence);
        self.bytes.extend(&b[16..]);
        if let Some(end) = self.bytes.iter().position(|c| *c == 0) {
            self.bytes.truncate(end);
            return Ok(true);
        }
        Ok(status == 0)
    }
}

fn diagnostic(t: &mut impl Transport, command: Command, delay: Duration) -> Result<String> {
    if !matches!(command, Command::Info | Command::ManualUs) {
        return Err("禁止未知命令".into());
    }
    done(t)?; // Consume old shared completion. Never run beside vendor debug tools.
    t.exchange(command)?; // Exactly one submission. No retry on any error below.
    let mut completed = false;
    for _ in 0..20 {
        thread::sleep(delay);
        if done(t)? {
            completed = true;
            break;
        }
    }
    if !completed {
        return Err("等待驱动超时：没有重发，实际状态未知；请稍后手动查看状态".into());
    }
    let mut out = Output::default();
    for _ in 0..64 {
        if out.push(&t.exchange(Command::Output)?)? {
            if !out.bytes.is_ascii() {
                return Err("输出含非 ASCII 数据".into());
            }
            return Ok(String::from_utf8(out.bytes).unwrap().replace('\r', ""));
        }
    }
    Err("输出超过限制；没有重发".into())
}

#[derive(Clone, Debug)]
pub struct Status {
    pub country: String,
    pub info: String,
}
impl Status {
    pub fn manual_us_supported(&self) -> bool {
        self.country == "US"
            && self.info.lines().any(|l| {
                let l = l.trim();
                l.starts_with("6G Support (domain:")
                    && !l.contains("domain:00")
                    && l.contains("REGU_RSN_MANUAL")
            })
    }
}

pub fn status(t: &mut impl Transport) -> Result<Status> {
    read_status(t, Duration::from_millis(100))
}
fn read_status(t: &mut impl Transport, delay: Duration) -> Result<Status> {
    let country = country(t)?;
    let info = diagnostic(t, Command::Info, delay)?;
    if !info.contains("6G Info") {
        return Err(format!("不是预期的 6 GHz 状态输出：{info}"));
    }
    Ok(Status { country, info })
}

pub struct TestResult {
    pub reply: Result<String>,
    pub after: Result<Status>,
    pub uncertain: bool,
}
pub fn set_us_once(t: &mut impl Transport) -> TestResult {
    set_and_check(t, Duration::from_millis(100))
}
fn set_and_check(t: &mut impl Transport, delay: Duration) -> TestResult {
    let reply = diagnostic(t, Command::ManualUs, delay);
    let known = reply.as_ref().is_ok_and(|s| {
        s.contains("Country code has changed to US") || s.contains("Invalid country code!")
    });
    if known {
        // Recheck even after an explicit rejection. Never infer success from US alone.
        let after = read_status(t, delay);
        let uncertain = after.is_err();
        TestResult {
            reply,
            after,
            uncertain,
        }
    } else {
        // Do not overwrite a possibly still-running diagnostic with another submit.
        let observed = country(t);
        TestResult {
            reply,
            after: Err(format!(
                "结果不确定；只复查国家码：{observed:?}。未覆盖诊断缓冲，未重发。"
            )),
            uncertain: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn block(text: &str, sequence: u32) -> Vec<u8> {
        let mut b = Vec::new();
        for v in [0, 0, sequence, text.len() as u32] {
            put(&mut b, v);
        }
        b.extend(text.as_bytes());
        b
    }
    #[test]
    fn layouts_and_bounds() {
        let p = packet(Command::ManualUs);
        assert_eq!(p.len(), 376);
        assert_eq!(word(&p, 24).unwrap(), 0xff818040);
        assert_eq!(
            &p[40..56],
            &[0, 0, 0, 0, 0, 0, 0, 0, 29, 0, 0, 0, 3, 0, 0, 0]
        );
        assert_eq!(&p[56..72], b"67\0\0\0\0\0\0\0\0\0\0\0\0\0\0");
        assert_eq!(&p[72..88], b"US\0\0\0\0\0\0\0\0\0\0\0\0\0\0");
        assert!(p[88..].iter().all(|b| *b == 0));
        let p = packet(Command::Country);
        let mut r = p.clone();
        r[28..32].copy_from_slice(&2u32.to_le_bytes());
        r[32..36].fill(0);
        r[40..42].copy_from_slice(b"US");
        assert_eq!(decode(&p, &r, r.len()).unwrap(), b"US");
        assert!(decode(&p, &p, p.len()).is_err());
        assert!(decode(&p, &r, 41).is_err());
        r[0] ^= 1;
        assert!(decode(&p, &r, r.len()).is_err());
        for len in 0..16 {
            assert!(Output::default().push(&vec![0; len]).is_err());
        }
        let mut output = Output::default();
        assert!(output.push(&block("abc\0padding", 1)).unwrap());
        assert_eq!(output.bytes, b"abc");
        assert!(output.push(&block("changed", 2)).is_err());
    }
    struct Mock {
        commands: Vec<Command>,
        fail: bool,
        reject: bool,
        current: Command,
    }
    impl Transport for Mock {
        fn exchange(&mut self, c: Command) -> Result<Vec<u8>> {
            self.commands.push(c);
            match c {
                Command::Country => Ok(b"US".to_vec()),
                Command::Done => Ok(1u32.to_le_bytes().to_vec()),
                Command::ManualUs if self.fail => Err("transport failed".into()),
                Command::ManualUs | Command::Info => {
                    self.current = c;
                    Ok(vec![])
                }
                Command::Output => Ok(block(
                    if self.current == Command::Info {
                        "6G Info\n6G Support (domain:05), due to REGU_RSN_MANUAL"
                    } else if self.reject {
                        "Invalid country code!"
                    } else {
                        "Country code has changed to US"
                    },
                    1,
                )),
            }
        }
    }
    #[test]
    fn one_write_and_recheck_including_rejection() {
        for (fail, reject) in [(false, false), (true, false), (false, true)] {
            let mut t = Mock {
                commands: vec![],
                fail,
                reject,
                current: Command::Info,
            };
            let result = set_and_check(&mut t, Duration::ZERO);
            assert_eq!(
                t.commands
                    .iter()
                    .filter(|c| **c == Command::ManualUs)
                    .count(),
                1
            );
            assert_eq!(t.commands.contains(&Command::Info), !fail);
            assert_eq!(result.uncertain, fail);
            if !fail {
                assert!(result.after.unwrap().manual_us_supported());
            }
        }
    }
    #[test]
    fn no_false_success() {
        for info in [
            "6G NOT Support",
            "6G Support (domain:00), due to REGU_RSN_MANUAL",
            "6G Support (domain:05), due to REGU_RSN_11D",
        ] {
            assert!(
                !Status {
                    country: "US".into(),
                    info: info.into()
                }
                .manual_us_supported()
            );
        }
    }
}
