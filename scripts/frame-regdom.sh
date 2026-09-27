#!/bin/sh

set -eu

hook=/etc/NetworkManager/dispatcher.d/pre-up.d/10-frame-regdom
iw=/usr/sbin/iw
marker='steam-frame-regdom: managed by scripts/frame-regdom.sh'

require_root() {
    if [ "$(id -u)" -ne 0 ]; then
        echo '请以 root 权限运行。' >&2
        exit 1
    fi
}

managed_hook() {
    [ -f "$hook" ] && [ ! -L "$hook" ] && grep -qF "$marker" "$hook"
}

case "${1:-}" in
    install)
        require_root
        [ -x "$iw" ] || { echo "找不到 $iw" >&2; exit 1; }
        [ -d "${hook%/*}" ] || { echo 'NetworkManager pre-up 目录不存在。' >&2; exit 1; }
        if [ -e "$hook" ] || [ -L "$hook" ]; then
            managed_hook || { echo "拒绝覆盖非本脚本管理的文件：$hook" >&2; exit 1; }
        fi
        tmp=$(mktemp "${hook}.XXXXXX")
        trap 'rm -f "$tmp"' 0 HUP INT TERM
        cat > "$tmp" <<'HOOK'
#!/bin/sh
# steam-frame-regdom: managed by scripts/frame-regdom.sh
if [ "$1" = wlan0 ] && [ "$2" = pre-up ]; then
    /usr/sbin/iw reg set US || { logger -t frame-regdom 'iw reg set US failed' || :; }
fi
exit 0
HOOK
        chmod 0755 "$tmp"
        if cmp -s "$tmp" "$hook"; then
            echo "已安装：$hook"
            exit 0
        fi
        mv -f -- "$tmp" "$hook"
        trap - 0 HUP INT TERM
        echo "已安装：$hook（下次 wlan0 连接前生效）"
        ;;
    status)
        if managed_hook; then
            echo "自动恢复：已安装（$hook）"
        else
            echo '自动恢复：未安装'
        fi
        "$iw" reg get
        ;;
    remove)
        require_root
        if [ ! -e "$hook" ] && [ ! -L "$hook" ]; then
            echo '自动恢复：未安装'
            exit 0
        fi
        managed_hook || { echo "拒绝删除非本脚本管理的文件：$hook" >&2; exit 1; }
        rm -- "$hook"
        echo '已卸载；当前国家码不会立即改变。'
        ;;
    *)
        echo "用法：$0 install|status|remove" >&2
        exit 2
        ;;
esac
