import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import { WatchAIIndicator } from './indicator.js';
import { WatchAIDbusClient } from './dbus_client.js';
import { SettingsManager } from './settings.js';

export default class WatchAIExtension extends Extension {
    _syncIndicator(state, activeCount) {
        if (!this._indicator) return;
        const sessions = this._indicator.popover ? this._indicator.popover.getSessions() : [];
        let workingCount = 0;
        let waitingCount = 0;
        let successCount = 0;
        let errorCount = 0;

        for (const s of sessions) {
            const st = (s.currentState || '').toUpperCase();
            if (st === 'WORKING') workingCount += 1;
            else if (st === 'WAITING') waitingCount += 1;
            else if (st === 'SUCCESS') successCount += 1;
            else if (st === 'ERROR') errorCount += 1;
        }

        const effectiveActive = activeCount !== undefined && activeCount !== null
            ? activeCount
            : (workingCount + waitingCount + successCount + errorCount);

        this._indicator.updateState(
            state || this._indicator._currentState || 'IDLE',
            effectiveActive,
            waitingCount,
            errorCount,
            workingCount,
            successCount
        );
    }

    enable() {
        // Initialize GSettings preferences manager (T063)
        this._settings = new SettingsManager(this);

        // Pass settings manager into top-bar indicator
        this._indicator = new WatchAIIndicator(this._settings);

        this._dbusClient = new WatchAIDbusClient({
            onConnected: (payload) => {
                if (!this._indicator) return;
                if (this._indicator.popover) {
                    this._indicator.popover.setOfflineMode(false);
                    if (payload && payload.sessions) {
                        this._indicator.popover.setSessions(payload.sessions);
                    }
                }
                const agg = payload ? payload.aggregateState : null;
                this._syncIndicator(agg ? agg.state : null, agg ? agg.activeCount : null);
            },
            onDisconnected: () => {
                if (this._indicator) {
                    this._indicator.setDisconnected();
                }
            },
            onAggregateStateChanged: (agg) => {
                if (this._indicator) {
                    if (this._dbusClient && typeof this._dbusClient.fetchSessions === 'function') {
                        this._dbusClient.fetchSessions((sessions) => {
                            if (this._indicator && this._indicator.popover && Array.isArray(sessions)) {
                                this._indicator.popover.setSessions(sessions);
                            }
                            this._syncIndicator(agg ? agg.state : null, agg ? agg.activeCount : null);
                        });
                    } else {
                        this._syncIndicator(agg ? agg.state : null, agg ? agg.activeCount : null);
                    }
                }
            },
            onSessionAdded: (session) => {
                if (this._indicator) {
                    if (this._indicator.popover) {
                        this._indicator.popover.addSession(session);
                    }
                    this._syncIndicator();
                    this._indicator.notifySessionUpdated(session);
                }
            },
            onSessionUpdated: (session) => {
                if (this._indicator) {
                    if (this._indicator.popover) {
                        this._indicator.popover.updateSession(session);
                    }
                    this._syncIndicator();
                    this._indicator.notifySessionUpdated(session);
                }
            },
            onSessionRemoved: (sessionId) => {
                if (this._indicator) {
                    if (this._indicator.popover) {
                        this._indicator.popover.removeSession(sessionId);
                    }
                    this._syncIndicator();
                    this._indicator.notifySessionRemoved(sessionId);
                }
            },
        });

        Main.panel.addToStatusArea('watchai-indicator', this._indicator);
    }

    disable() {
        if (this._dbusClient) {
            this._dbusClient.destroy();
            this._dbusClient = null;
        }

        if (this._indicator) {
            this._indicator.destroy();
            this._indicator = null;
        }

        if (this._settings) {
            this._settings.destroy();
            this._settings = null;
        }
    }
}
