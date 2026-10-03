import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

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
        this._ownerChangedId = null;
        this._reconnectTimer = null;
        this._isDestroyed = false;

        this._connect();
    }

    _connect() {
        if (this._isDestroyed) return;

        try {
            new WatchAIProxyWrapper(
                Gio.DBus.session,
                'org.freedesktop.WatchAI',
                '/org/freedesktop/WatchAI',
                (initProxy, error) => {
                    if (error) {
                        this._handleDisconnect();
                        return;
                    }
                    this._onProxyReady(initProxy);
                }
            );
        } catch (e) {
            this._handleDisconnect();
        }
    }

    _onProxyReady(proxy) {
        if (this._isDestroyed) return;

        this._proxy = proxy;

        // Watch for daemon disconnection / crashes
        this._ownerChangedId = this._proxy.connect('notify::g-name-owner', () => {
            if (!this._proxy.g_name_owner) {
                this._handleDisconnect();
            } else if (this._callbacks.onConnected) {
                this._callbacks.onConnected();
            }
        });

        // Listen for AggregateStateChanged signal
        this._signalId = this._proxy.connectSignal('AggregateStateChanged', (_proxy, _sender, params) => {
            if (this._callbacks.onAggregateStateChanged) {
                const [state, activeCount, waitingCount, errorCount, updatedAt] = params;
                this._callbacks.onAggregateStateChanged({
                    state,
                    activeCount,
                    waitingCount,
                    errorCount,
                    updatedAt,
                });
            }
        });

        // Initial fetch
        this.fetchAggregateState();
        if (this._callbacks.onConnected) {
            this._callbacks.onConnected();
        }
    }

    _handleDisconnect() {
        if (this._callbacks.onDisconnected) {
            this._callbacks.onDisconnected();
        }

        // Schedule reconnection attempt
        if (!this._reconnectTimer && !this._isDestroyed) {
            this._reconnectTimer = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 3, () => {
                this._reconnectTimer = null;
                this._connect();
                return GLib.SOURCE_REMOVE;
            });
        }
    }

    fetchAggregateState() {
        if (!this._proxy || !this._proxy.g_name_owner) return;

        this._proxy.GetAggregateStateRemote((result, error) => {
            if (error) return;
            if (this._callbacks.onAggregateStateChanged) {
                const [state, activeCount, waitingCount, errorCount, updatedAt] = result;
                this._callbacks.onAggregateStateChanged({
                    state,
                    activeCount,
                    waitingCount,
                    errorCount,
                    updatedAt,
                });
            }
        });
    }

    destroy() {
        this._isDestroyed = true;
        if (this._reconnectTimer) {
            GLib.source_remove(this._reconnectTimer);
            this._reconnectTimer = null;
        }

        if (this._proxy) {
            if (this._signalId) {
                this._proxy.disconnectSignal(this._signalId);
                this._signalId = null;
            }
            if (this._ownerChangedId) {
                this._proxy.disconnect(this._ownerChangedId);
                this._ownerChangedId = null;
            }
            this._proxy = null;
        }
    }
}
