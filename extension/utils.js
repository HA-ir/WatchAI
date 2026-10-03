import GLib from 'gi://GLib';

export function formatDuration(startedAtIso) {
    if (!startedAtIso) return '00:00';
    try {
        const start = GLib.DateTime.new_from_iso8601(startedAtIso, null);
        if (!start) return '00:00';
        const now = GLib.DateTime.new_now_utc();
        const diffSeconds = Math.max(0, now.to_unix() - start.to_unix());

        const hours = Math.floor(diffSeconds / 3600);
        const minutes = Math.floor((diffSeconds % 3600) / 60);
        const seconds = diffSeconds % 60;

        const pad = (n) => (n < 10 ? '0' + n : '' + n);
        if (hours > 0) {
            return `${pad(hours)}:${pad(minutes)}:${pad(seconds)}`;
        }
        return `${pad(minutes)}:${pad(seconds)}`;
    } catch {
        return '00:00';
    }
}

export function getStatePriority(state) {
    switch (state) {
        case 'ERROR': return 80;
        case 'WAITING': return 70;
        case 'WORKING': return 60;
        case 'STARTING': return 50;
        case 'CANCELLED': return 40;
        case 'SUCCESS': return 30;
        case 'UNKNOWN': return 20;
        case 'IDLE': return 10;
        default: return 0;
    }
}
