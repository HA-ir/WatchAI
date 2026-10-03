import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import {
    ConnectionState,
    HANDSHAKE_TIMEOUT_MS,
    computeBackoffDelay,
    unpackSessionDto,
} from './utils.js';

const WatchAIDbusInterface = `
<node>
  <interface name="org.freedesktop.WatchAI">
    <method name="GetAggregateState">
      <arg name="state" type="s" direction="out" />
      <arg name="active_session_count" type="u" direction="out" />
      <arg name="waiting_session_count" type="u" direction="out" />
      <arg name="error_session_count" type="u" direction="out" />
      <arg name="updated_at" type="s" direction="out" />
    </method>
    <method name="GetSessions">
      <arg name="sessions" type="a(sssssssus)" direction="out" />
    </method>
    <method name="GetSession">
      <arg name="session_id" type="s" direction="in" />
      <arg name="session" type="(sssssssus)" direction="out" />
    </method>
    <signal name="AggregateStateChanged">
      <arg name="state" type="s" />
      <arg name="active_session_count" type="u" />
      <arg name="waiting_session_count" type="u" />
      <arg name="error_session_count" type="u" />
      <arg name="updated_at" type="s" />
    </signal>
    <signal name="SessionAdded">
      <arg name="session" type="(sssssssus)" />
    </signal>
    <signal name="SessionUpdated">
      <arg name="session" type="(sssssssus)" />
    </signal>
    <signal name="SessionRemoved">
      <arg name="session_id" type="s" />
    </signal>
    <property name="AggregateState" type="s" access="read" />
    <property name="ActiveSessionCount" type="u" access="read" />
    <property name="DaemonVersion" type="s" access="read" />
  </interface>
</node>
`;

const WatchAIProxyWrapper = Gio.DBusProxy.makeProxyWrapper(WatchAIDbusInterface);

export class WatchAIDbusClient {
    constructor(callbacks = {}) {
        this._callbacks = callbacks;
        this._proxy = null;
        this._signalId = null;
        this._sessionAddedId = null;
        this._sessionUpdatedId = null;
        this._sessionRemovedId = null;
        this._ownerChangedId = null;
        this._reconnectTimer = null;
        this._handshakeTimer = null;
        this._consecutiveFailures = 0;
        this._isDestroyed = false;
        this._connectionState = ConnectionState.DISCONNECTED;

        this._connect();
    }

    get connectionState() {
        return this._connectionState;
    }

    _startHandshakeTimeout() {
        this._clearHandshakeTimeout();
        this._handshakeTimer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, HANDSHAKE_TIMEOUT_MS, () => {
            this._handshakeTimer = null;
            // Handshake exceeded 5.0 seconds -> abort and retry with backoff
            this._handleDisconnect();
            return GLib.SOURCE_REMOVE;
        });
    }

    _clearHandshakeTimeout() {
        if (this._handshakeTimer) {
            GLib.source_remove(this._handshakeTimer);
            this._handshakeTimer = null;
        }
    }

    _connect() {
        if (this._isDestroyed) return;

        this._connectionState = ConnectionState.CONNECTING;
        this._startHandshakeTimeout();

        try {
            new WatchAIProxyWrapper(
                Gio.DBus.session,
                'org.freedesktop.WatchAI',
                '/org/freedesktop/WatchAI',
                (initProxy, error) => {
                    if (error || !initProxy || !initProxy.g_name_owner) {
                        this._handleDisconnect();
                        return;
                    }
                    this._onProxyReady(initProxy);
                }
            );
        } catch {
            this._handleDisconnect();
        }
    }

    _onProxyReady(proxy) {
        if (this._isDestroyed) return;

        this._proxy = proxy;

        // Monitor D-Bus NameOwnerChanged for daemon departures and arrivals
        this._ownerChangedId = this._proxy.connect('notify::g-name-owner', () => {
            if (!this._proxy || !this._proxy.g_name_owner) {
                this._handleDisconnect();
            } else if (this._connectionState !== ConnectionState.CONNECTED) {
                // Daemon reappeared: cancel pending backoff and initiate handshake
                if (this._reconnectTimer) {
                    GLib.source_remove(this._reconnectTimer);
                    this._reconnectTimer = null;
                }
                this._connect();
            }
        });

        // Execute the 5-step synchronization handshake:
        // Step 1: Proxy acquired
        // Step 2: Query GetAggregateState
        this._proxy.GetAggregateStateRemote((aggResult, aggError) => {
            if (aggError || !aggResult) {
                this._handleDisconnect();
                return;
            }

            const [state, activeCount, waitingCount, errorCount, updatedAt] = aggResult;
            const aggregatePayload = {
                state: String(state || 'IDLE'),
                activeCount: Number(activeCount) || 0,
                waitingCount: Number(waitingCount) || 0,
                errorCount: Number(errorCount) || 0,
                updatedAt: String(updatedAt || ''),
            };

            // Step 3: Query GetSessions
            this._proxy.GetSessionsRemote((sessResult, sessError) => {
                if (sessError || !sessResult) {
                    this._handleDisconnect();
                    return;
                }

                const rawSessions = sessResult[0] || [];
                const sessions = rawSessions
                    .map(s => unpackSessionDto(s))
                    .filter(s => s !== null);

                // Step 4: Attach Signal Listeners
                this._attachSignalListeners();

                // Step 5: Mark Online, cancel handshake timer, reset failure counter
                this._clearHandshakeTimeout();
                this._consecutiveFailures = 0;
                this._connectionState = ConnectionState.CONNECTED;

                if (this._callbacks.onConnected) {
                    this._callbacks.onConnected({
                        aggregateState: aggregatePayload,
                        sessions,
                    });
                }
            });
        });
    }

    _attachSignalListeners() {
        if (!this._proxy) return;
        this._detachSignalListeners();

        // Listen for AggregateStateChanged signal
        this._signalId = this._proxy.connectSignal('AggregateStateChanged', (_proxy, _sender, params) => {
            if (this._callbacks.onAggregateStateChanged && params) {
                const [state, activeCount, waitingCount, errorCount, updatedAt] = params;
                this._callbacks.onAggregateStateChanged({
                    state: String(state || 'IDLE'),
                    activeCount: Number(activeCount) || 0,
                    waitingCount: Number(waitingCount) || 0,
                    errorCount: Number(errorCount) || 0,
                    updatedAt: String(updatedAt || ''),
                });
            }
        });

        // Listen for SessionAdded signal
        this._sessionAddedId = this._proxy.connectSignal('SessionAdded', (_proxy, _sender, params) => {
            if (this._callbacks.onSessionAdded && params && params[0]) {
                const session = unpackSessionDto(params[0]);
                if (session) {
                    this._callbacks.onSessionAdded(session);
                }
            }
        });

        // Listen for SessionUpdated signal
        this._sessionUpdatedId = this._proxy.connectSignal('SessionUpdated', (_proxy, _sender, params) => {
            if (this._callbacks.onSessionUpdated && params && params[0]) {
                const session = unpackSessionDto(params[0]);
                if (session) {
                    this._callbacks.onSessionUpdated(session);
                }
            }
        });

        // Listen for SessionRemoved signal
        this._sessionRemovedId = this._proxy.connectSignal('SessionRemoved', (_proxy, _sender, params) => {
            if (this._callbacks.onSessionRemoved && params && params[0]) {
                this._callbacks.onSessionRemoved(String(params[0]));
            }
        });
    }

    _detachSignalListeners() {
        if (!this._proxy) return;

        if (this._signalId) {
            this._proxy.disconnectSignal(this._signalId);
            this._signalId = null;
        }
        if (this._sessionAddedId) {
            this._proxy.disconnectSignal(this._sessionAddedId);
            this._sessionAddedId = null;
        }
        if (this._sessionUpdatedId) {
            this._proxy.disconnectSignal(this._sessionUpdatedId);
            this._sessionUpdatedId = null;
        }
        if (this._sessionRemovedId) {
            this._proxy.disconnectSignal(this._sessionRemovedId);
            this._sessionRemovedId = null;
        }
    }

    _handleDisconnect() {
        this._clearHandshakeTimeout();
        this._detachSignalListeners();

        if (this._ownerChangedId && this._proxy) {
            this._proxy.disconnect(this._ownerChangedId);
            this._ownerChangedId = null;
        }
        this._proxy = null;

        this._connectionState = ConnectionState.RECONNECTING;

        if (this._callbacks.onDisconnected) {
            this._callbacks.onDisconnected();
        }

        // Schedule reconnection attempt with jittered exponential backoff
        if (!this._reconnectTimer && !this._isDestroyed) {
            const delayMs = computeBackoffDelay(this._consecutiveFailures);
            this._consecutiveFailures += 1;

            this._reconnectTimer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, delayMs, () => {
                this._reconnectTimer = null;
                this._connect();
                return GLib.SOURCE_REMOVE;
            });
        }
    }

    fetchAggregateState() {
        if (!this._proxy || !this._proxy.g_name_owner) return;

        this._proxy.GetAggregateStateRemote((result, error) => {
            if (error || !result) return;
            if (this._callbacks.onAggregateStateChanged) {
                const [state, activeCount, waitingCount, errorCount, updatedAt] = result;
                this._callbacks.onAggregateStateChanged({
                    state: String(state || 'IDLE'),
                    activeCount: Number(activeCount) || 0,
                    waitingCount: Number(waitingCount) || 0,
                    errorCount: Number(errorCount) || 0,
                    updatedAt: String(updatedAt || ''),
                });
            }
        });
    }

    fetchSessions(callback) {
        if (!this._proxy || !this._proxy.g_name_owner) {
            if (callback) callback([]);
            return;
        }

        this._proxy.GetSessionsRemote((result, error) => {
            if (error || !result) {
                if (callback) callback([]);
                return;
            }
            const rawSessions = result[0] || [];
            const sessions = rawSessions
                .map(s => unpackSessionDto(s))
                .filter(s => s !== null);
            if (callback) callback(sessions);
        });
    }

    destroy() {
        this._isDestroyed = true;
        this._clearHandshakeTimeout();

        if (this._reconnectTimer) {
            GLib.source_remove(this._reconnectTimer);
            this._reconnectTimer = null;
        }

        this._detachSignalListeners();

        if (this._ownerChangedId && this._proxy) {
            this._proxy.disconnect(this._ownerChangedId);
            this._ownerChangedId = null;
        }

        this._proxy = null;
        this._connectionState = ConnectionState.DISCONNECTED;
    }
}
