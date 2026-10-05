// Unit test suite for WatchAI NotificationManager (Phase 10 - T134)

import { NotificationManager } from '../notifications.js';
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

function makeMockSession(id, state, provider = 'Claude Code', project = 'my-project') {
    return {
        sessionId: id,
        providerId: 'claude-code',
        providerDisplayName: provider,
        projectName: project,
        currentState: state,
        startedAt: '2026-10-06T12:00:00Z',
        stateEnteredAt: '2026-10-06T12:00:00Z',
        processId: 1234,
        activeToolCategory: 'Bash',
    };
}

function testTransitionEdgeDetection() {
    const notifications = [];
    const notifyFn = (title, body) => notifications.push({ title, body });
    const manager = new NotificationManager(null, notifyFn);

    // 1. Initial observation in WORKING -> No notification
    let res = manager.handleSessionUpdated(makeMockSession('s1', 'WORKING'));
    assertEqual(res, false, 'WORKING state must not notify');
    assertEqual(notifications.length, 0, 'No notification dispatched for WORKING');

    // 2. Transition WORKING -> WAITING -> Notifies!
    res = manager.handleSessionUpdated(makeMockSession('s1', 'WAITING'));
    assertEqual(res, true, 'Transition into WAITING must notify');
    assertEqual(notifications.length, 1, '1 notification dispatched');
    assertEqual(notifications[0].title, 'WatchAI: Claude Code (my-project)');
    assertEqual(notifications[0].body, 'Agent is waiting for user input or approval.');

    // 3. Transition WAITING -> WORKING -> No notification
    res = manager.handleSessionUpdated(makeMockSession('s1', 'WORKING'));
    assertEqual(res, false, 'Transition out of alert state must not notify');
    assertEqual(notifications.length, 1, 'Count remains 1');

    // 4. Transition WORKING -> SUCCESS -> No notification
    res = manager.handleSessionUpdated(makeMockSession('s1', 'SUCCESS'));
    assertEqual(res, false, 'SUCCESS must not notify');
    assertEqual(notifications.length, 1, 'Count remains 1');

    print('✓ Transition edge detection verified.');
}

function testIdenticalStateHeartbeatSuppression() {
    const notifications = [];
    const notifyFn = (title, body) => notifications.push({ title, body });
    const manager = new NotificationManager(null, notifyFn);

    // First transition into WAITING
    manager.handleSessionUpdated(makeMockSession('s1', 'WORKING'));
    manager.handleSessionUpdated(makeMockSession('s1', 'WAITING'));
    assertEqual(notifications.length, 1, 'Initial transition notifies');

    // Repeated identical state heartbeat (WAITING -> WAITING)
    const res = manager.handleSessionUpdated(makeMockSession('s1', 'WAITING'));
    assertEqual(res, false, 'Identical state WAITING -> WAITING must be suppressed');
    assertEqual(notifications.length, 1, 'Repeated heartbeat must not dispatch notification');

    print('✓ Identical-state heartbeat suppression verified.');
}

function testFiveSecondCooldownAndNoReplay() {
    const notifications = [];
    let currentTimeMs = 100000;
    const timeFn = () => currentTimeMs;
    const notifyFn = (title, body) => notifications.push({ title, body });
    const manager = new NotificationManager(null, notifyFn, timeFn);

    // T = 0s (100000ms): WORKING -> WAITING -> Notifies
    manager.handleSessionUpdated(makeMockSession('s1', 'WORKING'));
    let res = manager.handleSessionUpdated(makeMockSession('s1', 'WAITING'));
    assertEqual(res, true, 'First alert notifies at T=0');
    assertEqual(notifications.length, 1);

    // T = 1s (101000ms): WAITING -> WORKING
    currentTimeMs += 1000;
    manager.handleSessionUpdated(makeMockSession('s1', 'WORKING'));

    // T = 2s (102000ms): WORKING -> WAITING (elapsed 2000ms < 5000ms) -> SUPPRESSED
    currentTimeMs += 1000;
    res = manager.handleSessionUpdated(makeMockSession('s1', 'WAITING'));
    assertEqual(res, false, 'Rapid re-entry into WAITING within 2s must be suppressed');
    assertEqual(notifications.length, 1, 'Suppressed alert must not notify');

    // T = 4999ms (104999ms): WORKING -> WAITING (elapsed 4999ms < 5000ms) -> SUPPRESSED
    currentTimeMs = 104999;
    manager.handleSessionUpdated(makeMockSession('s1', 'WORKING'));
    res = manager.handleSessionUpdated(makeMockSession('s1', 'WAITING'));
    assertEqual(res, false, 'Alert at 4999ms (< 5000ms) must be suppressed');
    assertEqual(notifications.length, 1);

    // Prove NO delayed replay occurs automatically at cooldown expiration
    currentTimeMs = 105000;
    assertEqual(notifications.length, 1, 'Suppressed alert must NOT be queued or replayed after cooldown expires');

    // T = 5000ms exact boundary (105000ms): WAITING -> WORKING -> WAITING
    // elapsed = 105000 - 100000 = 5000ms (elapsed === 5000ms) -> ELIGIBLE!
    manager.handleSessionUpdated(makeMockSession('s1', 'WORKING'));
    res = manager.handleSessionUpdated(makeMockSession('s1', 'WAITING'));
    assertEqual(res, true, 'Alert at exactly 5000ms (elapsed === 5000ms) MUST be eligible and dispatch');
    assertEqual(notifications.length, 2, 'Second notification dispatched at exact 5000ms boundary');

    // T = 5000ms + 1000ms (106000ms): WAITING -> WORKING -> WAITING (< 5000ms since second alert) -> SUPPRESSED
    currentTimeMs += 1000;
    manager.handleSessionUpdated(makeMockSession('s1', 'WORKING'));
    res = manager.handleSessionUpdated(makeMockSession('s1', 'WAITING'));
    assertEqual(res, false, 'Alert 1000ms after second notification must be suppressed by new cooldown');
    assertEqual(notifications.length, 2);

    // T = 5000ms + 5001ms (110001ms): elapsed = 5001ms > 5000ms -> ELIGIBLE!
    currentTimeMs = 110001;
    manager.handleSessionUpdated(makeMockSession('s1', 'WORKING'));
    res = manager.handleSessionUpdated(makeMockSession('s1', 'WAITING'));
    assertEqual(res, true, 'Alert at 5001ms (> 5000ms) must notify');
    assertEqual(notifications.length, 3, 'Third notification dispatched');

    print('✓ Exact 5.0-second per-session cooldown boundary and no-queue/no-replay verified.');
}

function testPerSessionIsolation() {
    const notifications = [];
    const notifyFn = (title, body) => notifications.push({ title, body });
    const manager = new NotificationManager(null, notifyFn);

    // Session A enters WAITING -> Notifies
    manager.handleSessionUpdated(makeMockSession('sess-A', 'WORKING'));
    manager.handleSessionUpdated(makeMockSession('sess-A', 'WAITING', 'Claude Code', 'proj-a'));
    assertEqual(notifications.length, 1);
    assertEqual(notifications[0].title, 'WatchAI: Claude Code (proj-a)');

    // Session B enters WAITING 1 second later -> Notifies independently!
    manager.handleSessionUpdated(makeMockSession('sess-B', 'WORKING'));
    const resB = manager.handleSessionUpdated(makeMockSession('sess-B', 'WAITING', 'OpenCode', 'proj-b'));
    assertEqual(resB, true, 'Session B must NOT be throttled by Session A cooldown');
    assertEqual(notifications.length, 2);
    assertEqual(notifications[1].title, 'WatchAI: OpenCode (proj-b)');

    print('✓ Per-session cooldown isolation verified.');
}

function testSessionCleanupOnRemoval() {
    const notifications = [];
    let currentTimeMs = 100000;
    const timeFn = () => currentTimeMs;
    const notifyFn = (title, body) => notifications.push({ title, body });
    const manager = new NotificationManager(null, notifyFn, timeFn);

    // Session enters ERROR -> Notifies
    manager.handleSessionUpdated(makeMockSession('sess-cleanup', 'WORKING'));
    manager.handleSessionUpdated(makeMockSession('sess-cleanup', 'ERROR'));
    assertEqual(notifications.length, 1);

    // Remove session
    manager.cleanupSession('sess-cleanup');

    // Immediately after removal, new session with same ID appears in ERROR ->
    // Since state and cooldown were purged, it should evaluate cleanly as fresh
    const res = manager.handleSessionUpdated(makeMockSession('sess-cleanup', 'ERROR'));
    assertEqual(res, true, 'Cleaned session starts with fresh state history');
    assertEqual(notifications.length, 2);

    print('✓ Session removal cleanup verified.');
}

function testSettingsTogglesGating() {
    const fallback = new FallbackSettings();
    const settings = new SettingsManager(fallback);
    const notifications = [];
    const notifyFn = (title, body) => notifications.push({ title, body });
    const manager = new NotificationManager(settings, notifyFn);

    // 1. Master toggle disabled -> No notifications
    fallback.set_boolean('enable-desktop-notifications', false);
    manager.handleSessionUpdated(makeMockSession('s1', 'WORKING'));
    let res = manager.handleSessionUpdated(makeMockSession('s1', 'WAITING'));
    assertEqual(res, false, 'Master toggle false must suppress WAITING');
    assertEqual(notifications.length, 0);

    res = manager.handleSessionUpdated(makeMockSession('s2', 'ERROR'));
    assertEqual(res, false, 'Master toggle false must suppress ERROR');
    assertEqual(notifications.length, 0);

    // 2. Re-enable master, disable notify-on-waiting
    fallback.set_boolean('enable-desktop-notifications', true);
    fallback.set_boolean('notify-on-waiting', false);
    fallback.set_boolean('notify-on-error', true);

    res = manager.handleSessionUpdated(makeMockSession('s3', 'WAITING'));
    assertEqual(res, false, 'notify-on-waiting false must suppress WAITING');
    assertEqual(notifications.length, 0);

    res = manager.handleSessionUpdated(makeMockSession('s4', 'ERROR'));
    assertEqual(res, true, 'notify-on-error true must allow ERROR');
    assertEqual(notifications.length, 1);
    assertEqual(notifications[0].body, 'Agent encountered an error or crashed.');

    print('✓ Settings toggles gating verified.');
}

function testProjectNameSanitizationAndPrivacy() {
    const notifications = [];
    const notifyFn = (title, body) => notifications.push({ title, body });
    const manager = new NotificationManager(null, notifyFn);

    // Adversarial project names with newlines, control chars, and markup
    const pathologicalProject = 'secret\n<script>alert(1)</script>\t&dangerous-very-long-project-name-exceeding-thirty-two-chars';
    manager.handleSessionUpdated(makeMockSession('s1', 'WAITING', 'Claude Code', pathologicalProject));

    assertEqual(notifications.length, 1);
    const title = notifications[0].title;

    // Must not contain newlines, tabs, or markup characters
    assert(!title.includes('\n'), 'Title must not contain newline');
    assert(!title.includes('\t'), 'Title must not contain tab');
    assert(!title.includes('<'), 'Title must not contain <');
    assert(!title.includes('>'), 'Title must not contain >');
    assert(!title.includes('&'), 'Title must not contain &');

    // Body must strictly be generic prompt-free text
    assertEqual(notifications[0].body, 'Agent is waiting for user input or approval.');

    // Empty/whitespace-only project name falls back to 'workspace'
    manager.handleSessionUpdated(makeMockSession('s2', 'WAITING', 'OpenAI Codex', '   \t\n  '));
    assertEqual(notifications[1].title, 'WatchAI: OpenAI Codex (workspace)');

    // Remediation 3: Unicode boundary test with emoji across the 32-character boundary
    // 31 ASCII chars + '🚀' (2 code units, 1 code point) -> exactly 32 code points (33 code units)
    const base31 = 'abcdefghijklmnopqrstuvwxyz12345';
    assertEqual(base31.length, 31, 'base31 length');
    const projectWithEmojiAt32 = base31 + '🚀';

    manager.handleSessionUpdated(makeMockSession('s3', 'WAITING', 'OpenCode', projectWithEmojiAt32));
    const titleWithEmoji = notifications[2].title;
    assert(titleWithEmoji.includes('🚀'), 'Emoji at code point 32 must be preserved intact');

    // Verify no unpaired UTF-16 surrogate exists in the title
    const hasLoneSurrogate = /[\uD800-\uDBFF](?![\uDC00-\uDFFF])|(?<![\uD800-\uDBFF])[\uDC00-\uDFFF]/.test(titleWithEmoji);
    assert(!hasLoneSurrogate, 'Notification title must never contain an unpaired UTF-16 surrogate');

    // 32 ASCII chars + '🚀' -> 33 code points, must truncate to exactly 32 code points without surrogate residue
    const base32 = 'abcdefghijklmnopqrstuvwxyz123456';
    const projectExceedingByEmoji = base32 + '🚀';
    manager.handleSessionUpdated(makeMockSession('s4', 'WAITING', 'Claude Code', projectExceedingByEmoji));
    const titleExceeding = notifications[3].title;
    assert(!titleExceeding.includes('🚀'), 'Emoji exceeding 32 code points must be cleanly dropped');
    const hasLoneSurrogate2 = /[\uD800-\uDBFF](?![\uDC00-\uDFFF])|(?<![\uD800-\uDBFF])[\uDC00-\uDFFF]/.test(titleExceeding);
    assert(!hasLoneSurrogate2, 'Truncated title must not contain lone surrogate');

    print('✓ Project name sanitization and privacy boundaries verified.');
}

try {
    testTransitionEdgeDetection();
    testIdenticalStateHeartbeatSuppression();
    testFiveSecondCooldownAndNoReplay();
    testPerSessionIsolation();
    testSessionCleanupOnRemoval();
    testSettingsTogglesGating();
    testProjectNameSanitizationAndPrivacy();
    print('\nAll NotificationManager GJS tests passed successfully!');
} catch (e) {
    printerr('Test failed: ' + e + '\n' + e.stack);
    imports.system.exit(1);
}
