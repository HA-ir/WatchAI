export const SCHEMA_ID = 'org.gnome.shell.extensions.watchai';

export const DEFAULT_PREFERENCES = {
    'dwell-duration-seconds': 60,
    'enable-desktop-notifications': true,
    'notify-on-waiting': true,
    'notify-on-error': true,
    'indicator-icon-style': 'symbolic',
};

/**
 * In-memory fallback implementation of GSettings used during headless test runs
 * or when gschemas.compiled is not yet generated in source-tree environments.
 */
export class FallbackSettings {
    constructor(initialValues = {}) {
        this._values = Object.assign({}, DEFAULT_PREFERENCES, initialValues);
        this._listeners = new Map(); // signalName -> Map(id, callback)
        this._nextListenerId = 1;
    }

    get_uint(key) {
        const val = this._values[key];
        return typeof val === 'number' ? val : (DEFAULT_PREFERENCES[key] || 0);
    }

    set_uint(key, val) {
        this._values[key] = Number(val);
        this._emitChanged(key);
    }

    get_boolean(key) {
        const val = this._values[key];
        return typeof val === 'boolean' ? val : Boolean(DEFAULT_PREFERENCES[key]);
    }

    set_boolean(key, val) {
        this._values[key] = Boolean(val);
        this._emitChanged(key);
    }

    get_string(key) {
        const val = this._values[key];
        return typeof val === 'string' ? val : String(DEFAULT_PREFERENCES[key] || '');
    }

    set_string(key, val) {
        this._values[key] = String(val);
        this._emitChanged(key);
    }

    connect(signal, callback) {
        const id = this._nextListenerId++;
        if (!this._listeners.has(signal)) {
            this._listeners.set(signal, new Map());
        }
        this._listeners.get(signal).set(id, callback);
        return id;
    }

    disconnect(id) {
        for (const map of this._listeners.values()) {
            if (map.has(id)) {
                map.delete(id);
                return;
            }
        }
    }

    _emitChanged(key) {
        const specificSignal = `changed::${key}`;
        const generalSignal = 'changed';

        if (this._listeners.has(specificSignal)) {
            for (const cb of this._listeners.get(specificSignal).values()) {
                cb(this, key);
            }
        }
        if (this._listeners.has(generalSignal)) {
            for (const cb of this._listeners.get(generalSignal).values()) {
                cb(this, key);
            }
        }
    }
}

/**
 * Manages GSettings bindings, typed preference accessors, and signal listeners.
 * Defensively falls back to in-memory defaults if schemas are not compiled
 * in development/test setups, while strictly propagating unexpected runtime errors.
 */
export class SettingsManager {
    constructor(extensionOrSettings = null) {
        this._isFallback = false;
        this._listenerIds = []; // Array of { id, settings }

        if (!extensionOrSettings) {
            console.warn('WatchAI: No settings or extension provided; using in-memory fallback settings.');
            this._settings = new FallbackSettings();
            this._isFallback = true;
            return;
        }

        // 1. Direct GSettings or FallbackSettings passed
        if (typeof extensionOrSettings.get_string === 'function' &&
            typeof extensionOrSettings.connect === 'function') {
            this._settings = extensionOrSettings;
            this._isFallback = extensionOrSettings instanceof FallbackSettings;
            return;
        }

        // 2. GNOME Shell Extension instance passed
        if (typeof extensionOrSettings.getSettings === 'function') {
            try {
                this._settings = extensionOrSettings.getSettings();
                this._isFallback = false;
            } catch (err) {
                // Determine whether error is due to missing schema file
                // Strictly limited to absence signatures; syntax or permission errors rethrow
                const errStr = String(err && (err.message || err)).toLowerCase();
                const isSchemaMissing = errStr.includes('not found') ||
                                        errStr.includes('not installed') ||
                                        errStr.includes('is not installed') ||
                                        errStr.includes('no such file') ||
                                        errStr.includes('does not exist');

                if (isSchemaMissing) {
                    console.warn(`WatchAI: GSettings schema '${SCHEMA_ID}' not found; activating fallback settings.`);
                    this._settings = new FallbackSettings();
                    this._isFallback = true;
                } else {
                    // Do NOT swallow unrelated runtime, syntax, or corruption errors
                    throw err;
                }
            }
            return;
        }

        // Unknown object passed
        console.warn('WatchAI: Unrecognized settings source; using in-memory fallback.');
        this._settings = new FallbackSettings();
        this._isFallback = true;
    }

    isUsingFallback() {
        return this._isFallback;
    }

    get rawSettings() {
        return this._settings;
    }

    getDwellDurationSeconds() {
        try {
            const val = this._settings.get_uint('dwell-duration-seconds');
            return (val >= 1 && val <= 60) ? val : 10;
        } catch {
            return 10;
        }
    }

    getEnableNotifications() {
        try {
            return this._settings.get_boolean('enable-desktop-notifications');
        } catch {
            return true;
        }
    }

    getNotifyOnWaiting() {
        try {
            return this._settings.get_boolean('notify-on-waiting');
        } catch {
            return true;
        }
    }

    getNotifyOnError() {
        try {
            return this._settings.get_boolean('notify-on-error');
        } catch {
            return true;
        }
    }

    getIconStyle() {
        try {
            const val = this._settings.get_string('indicator-icon-style');
            if (val === 'colored') {
                return 'colored';
            }
            // Deterministic fallback to 'symbolic' for any unrecognized string
            return 'symbolic';
        } catch {
            return 'symbolic';
        }
    }

    /**
     * Connect a listener to a specific key change.
     * Automatically tracked for clean disconnection in destroy().
     */
    onChanged(key, callback) {
        if (!this._settings || typeof this._settings.connect !== 'function') {
            return 0;
        }
        const signal = `changed::${key}`;
        const id = this._settings.connect(signal, callback);
        this._listenerIds.push({ id, settings: this._settings });
        return id;
    }

    /**
     * Disconnect a single tracked listener by its ID.
     * Harmless if the ID is unknown, already disconnected, or settings is destroyed.
     */
    disconnect(id) {
        if (!id || !this._settings) return;
        const idx = this._listenerIds.findIndex(item => item.id === id);
        if (idx !== -1) {
            const { id: listenerId, settings } = this._listenerIds[idx];
            try {
                if (settings && typeof settings.disconnect === 'function') {
                    settings.disconnect(listenerId);
                }
            } catch {
                // Ignore errors during disposal
            }
            this._listenerIds.splice(idx, 1);
        }
    }

    /**
     * Cleanly disconnect all registered listeners.
     */
    destroy() {
        for (const { id, settings } of this._listenerIds) {
            try {
                if (settings && typeof settings.disconnect === 'function') {
                    settings.disconnect(id);
                }
            } catch {
                // Ignore errors during disposal
            }
        }
        this._listenerIds = [];
        this._settings = null;
    }
}
