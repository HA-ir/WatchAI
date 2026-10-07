// Unit test suite for WatchAI SettingsManager (Phase 10 - T129)

import { SettingsManager, FallbackSettings } from '../settings.js';

function assert(condition, message) {
    if (!condition) {
        throw new Error(message || 'Assertion failed');
    }
}

function assertEqual(actual, expected, message) {
    if (actual !== expected) {
        throw new Error(`${message || 'Assertion failed'}: expected ${expected}, got ${actual}`);
    }
}

function testFallbackSettingsDefaults() {
    const fallback = new FallbackSettings();
    assertEqual(fallback.get_uint('dwell-duration-seconds'), 60, 'Default dwell duration must be 60');
    assertEqual(fallback.get_boolean('enable-desktop-notifications'), true, 'Default notifications must be true');
    assertEqual(fallback.get_boolean('notify-on-waiting'), true, 'Default notify-on-waiting must be true');
    assertEqual(fallback.get_boolean('notify-on-error'), true, 'Default notify-on-error must be true');
    assertEqual(fallback.get_string('indicator-icon-style'), 'symbolic', 'Default icon style must be symbolic');
    print('✓ FallbackSettings defaults verified.');
}

function testSettingsManagerTypedGetters() {
    const fallback = new FallbackSettings();
    const mgr = new SettingsManager(fallback);

    assertEqual(mgr.getDwellDurationSeconds(), 60, 'Getter for dwell duration');
    assertEqual(mgr.getEnableNotifications(), true, 'Getter for enable notifications');
    assertEqual(mgr.getNotifyOnWaiting(), true, 'Getter for notify-on-waiting');
    assertEqual(mgr.getNotifyOnError(), true, 'Getter for notify-on-error');
    assertEqual(mgr.getIconStyle(), 'symbolic', 'Getter for icon style');
    print('✓ SettingsManager typed getters verified.');
}

function testInvalidIconStyleFallback() {
    const fallback = new FallbackSettings();
    const mgr = new SettingsManager(fallback);

    fallback.set_string('indicator-icon-style', 'colored');
    assertEqual(mgr.getIconStyle(), 'colored', 'Icon style colored must be returned');

    // Invalid/unrecognized string value
    fallback.set_string('indicator-icon-style', 'neon-glow');
    assertEqual(mgr.getIconStyle(), 'symbolic', 'Invalid icon style must deterministically fall back to symbolic');

    fallback.set_string('indicator-icon-style', '');
    assertEqual(mgr.getIconStyle(), 'symbolic', 'Empty icon style must fall back to symbolic');
    print('✓ Invalid indicator-icon-style fallback to symbolic verified.');
}

function testDynamicChangedListeners() {
    const fallback = new FallbackSettings();
    const mgr = new SettingsManager(fallback);

    let waitingNotificationFired = false;
    let newWaitingVal = null;
    mgr.onChanged('notify-on-waiting', (_settings, key) => {
        waitingNotificationFired = true;
        newWaitingVal = mgr.getNotifyOnWaiting();
    });

    let iconStyleFired = false;
    let newIconStyle = null;
    mgr.onChanged('indicator-icon-style', (_settings, key) => {
        iconStyleFired = true;
        newIconStyle = mgr.getIconStyle();
    });

    // Mutate notify-on-waiting
    fallback.set_boolean('notify-on-waiting', false);
    assert(waitingNotificationFired, 'Changed listener for notify-on-waiting must fire');
    assertEqual(newWaitingVal, false, 'Updated notify-on-waiting value must be false');

    // Mutate indicator-icon-style
    fallback.set_string('indicator-icon-style', 'colored');
    assert(iconStyleFired, 'Changed listener for indicator-icon-style must fire');
    assertEqual(newIconStyle, 'colored', 'Updated icon style must be colored');
    print('✓ Dynamic changed:: signal dispatch verified.');
}

function testCleanTeardownAndDisconnection() {
    const fallback = new FallbackSettings();
    const mgr = new SettingsManager(fallback);

    let firedCount = 0;
    mgr.onChanged('enable-desktop-notifications', () => {
        firedCount++;
    });

    fallback.set_boolean('enable-desktop-notifications', false);
    assertEqual(firedCount, 1, 'Listener should have fired once');

    // Destroy manager
    mgr.destroy();

    // Mutate again after destroy
    fallback.set_boolean('enable-desktop-notifications', true);
    assertEqual(firedCount, 1, 'Listener must NOT fire after destroy() is called');
    print('✓ Clean listener disconnection on destroy() verified.');
}

function testMissingSchemaFallbackGracefulHandling() {
    // Construct with null or extension that throws schema error
    const throwingExtension = {
        getSettings() {
            const err = new Error('Settings schema org.gnome.shell.extensions.watchai not found');
            err.name = 'GLib.Error';
            throw err;
        }
    };

    const mgr = new SettingsManager(throwingExtension);
    assert(mgr.isUsingFallback(), 'Must activate fallback when schema is missing');
    assertEqual(mgr.getIconStyle(), 'symbolic', 'Fallback returns valid default');
    print('✓ Missing schema fallback activates gracefully without throwing.');
}

function testUnrelatedExceptionNotSwallowed() {
    const buggyExtension = {
        getSettings() {
            throw new TypeError('Cannot read property foo of undefined');
        }
    };

    let caught = false;
    try {
        new SettingsManager(buggyExtension);
    } catch (e) {
        if (e instanceof TypeError) {
            caught = true;
        }
    }
    assert(caught, 'Unrelated TypeErrors must be re-thrown and not swallowed');
    print('✓ Unrelated runtime errors are not swallowed by fallback.');
}

function testIndividualListenerDisconnect() {
    const fallback = new FallbackSettings();
    const mgr = new SettingsManager(fallback);

    let count1 = 0;
    let count2 = 0;

    const id1 = mgr.onChanged('indicator-icon-style', () => { count1++; });
    const id2 = mgr.onChanged('indicator-icon-style', () => { count2++; });

    // Mutate: both should fire
    fallback.set_string('indicator-icon-style', 'colored');
    assertEqual(count1, 1, 'Listener 1 should fire');
    assertEqual(count2, 1, 'Listener 2 should fire');

    // Disconnect listener 1 only
    mgr.disconnect(id1);

    // Mutate again: only listener 2 should fire
    fallback.set_string('indicator-icon-style', 'symbolic');
    assertEqual(count1, 1, 'Disconnected listener 1 must NOT fire again');
    assertEqual(count2, 2, 'Active listener 2 must fire again');

    // Disconnecting unknown or already-disconnected ID must be harmless
    mgr.disconnect(99999);
    mgr.disconnect(id1);
    mgr.disconnect(null);

    // Final destroy cleans everything
    mgr.destroy();
    fallback.set_string('indicator-icon-style', 'colored');
    assertEqual(count2, 2, 'Listener 2 must not fire after destroy()');

    print('✓ Individual listener disconnect(id) and clean isolation verified.');
}

function testSchemaSyntaxErrorRethrown() {
    // Malformed schema or syntax errors must be rethrown, NOT masked by fallback
    const malformedExtension = {
        getSettings() {
            throw new Error('Schema XML syntax error: line 5: unexpected closing tag');
        }
    };

    let caught = false;
    try {
        new SettingsManager(malformedExtension);
    } catch (e) {
        if (e.message && e.message.includes('Schema XML syntax error')) {
            caught = true;
        }
    }
    assert(caught, 'Schema XML syntax error must be re-thrown and not masked by fallback');

    // Corrupt schema file must also be rethrown
    const corruptExtension = {
        getSettings() {
            throw new Error('Corrupt schema file: invalid header in gschemas.compiled');
        }
    };
    let corruptCaught = false;
    try {
        new SettingsManager(corruptExtension);
    } catch (e) {
        if (e.message && e.message.includes('Corrupt schema file')) {
            corruptCaught = true;
        }
    }
    assert(corruptCaught, 'Corrupt schema error must be re-thrown and not masked by fallback');

    print('✓ Schema syntax and corruption errors are rethrown and not masked.');
}

try {
    testFallbackSettingsDefaults();
    testSettingsManagerTypedGetters();
    testInvalidIconStyleFallback();
    testDynamicChangedListeners();
    testCleanTeardownAndDisconnection();
    testIndividualListenerDisconnect();
    testMissingSchemaFallbackGracefulHandling();
    testUnrelatedExceptionNotSwallowed();
    testSchemaSyntaxErrorRethrown();
    print('\nAll SettingsManager GJS tests passed successfully!');
} catch (e) {
    printerr('Test failed: ' + e + '\n' + e.stack);
    imports.system.exit(1);
}
