import { sanitizeProjectName } from './utils.js';

export const COOLDOWN_MS = 5000;

let _Main = null;

async function dispatchNativeNotification(title, body) {
    if (!_Main) {
        if (typeof globalThis.Main !== 'undefined') {
            _Main = globalThis.Main;
        } else {
            try {
                _Main = await import('resource:///org/gnome/shell/ui/main.js');
            } catch {
                _Main = null;
            }
        }
    }

    if (_Main && typeof _Main.notify === 'function') {
        _Main.notify(title, body);
    } else {
        console.warn('WatchAI: GNOME Shell Main.notify not available in current environment.');
    }
}

/**
 * Manages native desktop notifications for AI agent sessions.
 * Enforces edge-triggered alerts (WAITING, ERROR), a strict 5.0s per-session cooldown,
 * immediate suppression without queueing, per-session isolation, and zero-leakage privacy.
 */
export class NotificationManager {
    constructor(settings = null, notifyFn = null, timeFn = null) {
        this._settings = settings;
        this._timeFn = typeof timeFn === 'function' ? timeFn : () => Date.now();
        this._notifyFn = typeof notifyFn === 'function' ? notifyFn : (title, body) => {
            dispatchNativeNotification(title, body).catch(err => {
                console.warn('WatchAI: Failed to dispatch native desktop notification:', err);
            });
        };

        this._lastStates = new Map();     // sessionId -> previousState
        this._lastNotifiedAt = new Map(); // sessionId -> timestampMs
    }

    /**
     * Evaluate incoming session update for notification dispatch.
     * Returns true if a notification was dispatched; false if ignored or suppressed.
     */
    handleSessionUpdated(session) {
        if (!session || !session.sessionId) {
            return false;
        }

        const sessionId = session.sessionId;
        const currentState = session.currentState;
        const prevState = this._lastStates.get(sessionId);

        // Record current state for transition history
        this._lastStates.set(sessionId, currentState);

        // 1. Alert State Gate: Only transitions into WAITING or ERROR can notify
        if (currentState !== 'WAITING' && currentState !== 'ERROR') {
            return false;
        }

        // 2. Transition Edge Gate: Repeated identical states (heartbeats) never notify
        if (prevState === currentState) {
            return false;
        }

        // 3. Settings Toggles Gate
        if (this._settings) {
            if (!this._settings.getEnableNotifications()) {
                return false;
            }
            if (currentState === 'WAITING' && !this._settings.getNotifyOnWaiting()) {
                return false;
            }
            if (currentState === 'ERROR' && !this._settings.getNotifyOnError()) {
                return false;
            }
        }

        // 4. Cooldown Gate (5.0s per session): Suppress immediately inside cooldown (no queueing / no replay)
        const nowMs = this._timeFn();
        const lastNotified = this._lastNotifiedAt.get(sessionId) || 0;
        if (nowMs - lastNotified < COOLDOWN_MS) {
            return false;
        }

        // All gates passed: Eligible notification!
        this._lastNotifiedAt.set(sessionId, nowMs);

        const providerName = session.providerDisplayName || session.providerId || 'AI Agent';
        const projectName = sanitizeProjectName(session.projectName);
        const title = `WatchAI: ${providerName} (${projectName})`;

        const body = currentState === 'WAITING'
            ? 'Agent is waiting for user input or approval.'
            : 'Agent encountered an error or crashed.';

        try {
            this._notifyFn(title, body);
            return true;
        } catch (err) {
            console.warn('WatchAI: Error invoking notification callback:', err);
            return false;
        }
    }

    /**
     * Purge all tracking state for a removed session to prevent memory leaks.
     */
    cleanupSession(sessionId) {
        if (sessionId) {
            this._lastStates.delete(sessionId);
            this._lastNotifiedAt.delete(sessionId);
        }
    }

    /**
     * Clean up all internal maps and references.
     */
    destroy() {
        this._lastStates.clear();
        this._lastNotifiedAt.clear();
        this._settings = null;
        this._notifyFn = null;
    }
}
