import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import { WatchAIIndicator } from './indicator.js';
import { WatchAIDbusClient } from './dbus_client.js';

export default class WatchAIExtension extends Extension {
    enable() {
        this._indicator = new WatchAIIndicator();

        this._dbusClient = new WatchAIDbusClient({
            onConnected: (payload) => {
                if (!this._indicator) return;
                if (payload && payload.aggregateState) {
                    this._indicator.updateState(
                        payload.aggregateState.state,
                        payload.aggregateState.activeCount,
                        payload.aggregateState.waitingCount,
                        payload.aggregateState.errorCount
                    );
                }
                if (this._indicator.popover) {
                    this._indicator.popover.setOfflineMode(false);
                    if (payload && payload.sessions) {
                        this._indicator.popover.setSessions(payload.sessions);
                    }
                }
            },
            onDisconnected: () => {
                if (this._indicator) {
                    this._indicator.setDisconnected();
                }
            },
            onAggregateStateChanged: (agg) => {
                if (this._indicator) {
                    this._indicator.updateState(
                        agg.state,
                        agg.activeCount,
                        agg.waitingCount,
                        agg.errorCount
                    );
                }
            },
            onSessionAdded: (session) => {
                if (this._indicator && this._indicator.popover) {
                    this._indicator.popover.addSession(session);
                }
            },
            onSessionUpdated: (session) => {
                if (this._indicator && this._indicator.popover) {
                    this._indicator.popover.updateSession(session);
                }
            },
            onSessionRemoved: (sessionId) => {
                if (this._indicator && this._indicator.popover) {
                    this._indicator.popover.removeSession(sessionId);
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
    }
}
