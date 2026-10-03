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

export const ConnectionState = {
    DISCONNECTED: 'DISCONNECTED',
    CONNECTING: 'CONNECTING',
    CONNECTED: 'CONNECTED',
    RECONNECTING: 'RECONNECTING',
};

export const HANDSHAKE_TIMEOUT_MS = 5000;
export const INITIAL_INTERVAL_MS = 1000;
export const MAX_INTERVAL_MS = 30000;
export const MULTIPLIER = 2.0;
export const JITTER_RATIO = 0.20;

/// Compute exponential backoff delay with ±20% jitter.
export function computeBackoffDelay(consecutiveFailures, randomFn = Math.random) {
    const base = Math.min(
        MAX_INTERVAL_MS,
        INITIAL_INTERVAL_MS * Math.pow(MULTIPLIER, consecutiveFailures)
    );
    const minJitter = 1.0 - JITTER_RATIO;
    const maxJitter = 1.0 + JITTER_RATIO;
    const factor = minJitter + (maxJitter - minJitter) * randomFn();
    return Math.round(base * factor);
}

/// Defensively unpack a D-Bus session struct tuple (sssssssus) into an object.
/// Returns null if the payload is malformed or invalid.
export function unpackSessionDto(s) {
    if (!Array.isArray(s) || s.length < 9) {
        return null;
    }
    return {
        sessionId: String(s[0] || ''),
        providerId: String(s[1] || ''),
        providerDisplayName: String(s[2] || ''),
        projectName: String(s[3] || ''),
        currentState: String(s[4] || 'IDLE'),
        startedAt: String(s[5] || ''),
        stateEnteredAt: String(s[6] || ''),
        processId: Number(s[7]) || 0,
        activeToolCategory: String(s[8] || ''),
    };
}
