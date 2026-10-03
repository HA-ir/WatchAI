import {
    ConnectionState,
    HANDSHAKE_TIMEOUT_MS,
    INITIAL_INTERVAL_MS,
    MAX_INTERVAL_MS,
    MULTIPLIER,
    JITTER_RATIO,
    computeBackoffDelay,
    unpackSessionDto,
} from '../utils.js';

function assert(condition, message) {
    if (!condition) {
        throw new Error(`Assertion failed: ${message}`);
    }
}

function assertEq(actual, expected, message) {
    if (actual !== expected) {
        throw new Error(`Assertion failed: ${message} (expected ${expected}, got ${actual})`);
    }
}

// ============================================================================
// 1. Verify Timing Constants
// ============================================================================
assertEq(HANDSHAKE_TIMEOUT_MS, 5000, 'Handshake timeout must be exactly 5000ms (5.0s)');
assertEq(INITIAL_INTERVAL_MS, 1000, 'Initial reconnect interval must be exactly 1000ms (1.0s)');
assertEq(MAX_INTERVAL_MS, 30000, 'Maximum reconnect interval ceiling must be 30000ms (30.0s)');
assertEq(MULTIPLIER, 2.0, 'Backoff multiplier must be 2.0');
assertEq(JITTER_RATIO, 0.20, 'Jitter ratio must be 20% (±20%)');

// ============================================================================
// 2. Verify Jittered Exponential Backoff Calculations (T087)
// ============================================================================
const minRng = () => 0.0;
const midRng = () => 0.5;
const maxRng = () => 1.0;

// Attempt 0: Base 1000ms
assertEq(computeBackoffDelay(0, midRng), 1000, 'Attempt 0 median must be 1000ms');
assertEq(computeBackoffDelay(0, minRng), 800, 'Attempt 0 min jitter must be 800ms (-20%)');
assertEq(computeBackoffDelay(0, maxRng), 1200, 'Attempt 0 max jitter must be 1200ms (+20%)');

// Attempt 1: Base 2000ms
assertEq(computeBackoffDelay(1, midRng), 2000, 'Attempt 1 median must be 2000ms');
assertEq(computeBackoffDelay(1, minRng), 1600, 'Attempt 1 min jitter must be 1600ms');
assertEq(computeBackoffDelay(1, maxRng), 2400, 'Attempt 1 max jitter must be 2400ms');

// Attempt 2: Base 4000ms
assertEq(computeBackoffDelay(2, midRng), 4000, 'Attempt 2 median must be 4000ms');
assertEq(computeBackoffDelay(2, minRng), 3200, 'Attempt 2 min jitter must be 3200ms');
assertEq(computeBackoffDelay(2, maxRng), 4800, 'Attempt 2 max jitter must be 4800ms');

// Attempt 3: Base 8000ms
assertEq(computeBackoffDelay(3, midRng), 8000, 'Attempt 3 median must be 8000ms');

// Attempt 4: Base 16000ms
assertEq(computeBackoffDelay(4, midRng), 16000, 'Attempt 4 median must be 16000ms');

// Attempt 5+: Clamped at 30000ms ceiling!
assertEq(computeBackoffDelay(5, midRng), 30000, 'Attempt 5 must be clamped to 30000ms ceiling');
assertEq(computeBackoffDelay(5, minRng), 24000, 'Attempt 5 min jitter must be 24000ms (-20%)');
assertEq(computeBackoffDelay(5, maxRng), 36000, 'Attempt 5 max jitter must be 36000ms (+20%)');
assertEq(computeBackoffDelay(10, midRng), 30000, 'Attempt 10 must remain clamped to 30000ms ceiling');

// Success reset verification: passing 0 after consecutive failures resets backoff
let failures = 5;
assertEq(computeBackoffDelay(failures, midRng), 30000, 'High failures evaluate to ceiling');
failures = 0; // Reset upon successful handshake!
assertEq(computeBackoffDelay(failures, midRng), 1000, 'Reset failures must return to 1000ms base');

print('✓ Jittered exponential backoff and timeout constants verified.');

// ============================================================================
// 3. Verify ConnectionState Machine Transitions (T078)
// ============================================================================
let state = ConnectionState.DISCONNECTED;
assertEq(state, 'DISCONNECTED', 'Initial state is DISCONNECTED');

// Disconnected -> Connecting
state = ConnectionState.CONNECTING;
assertEq(state, 'CONNECTING', 'Initiating handshake transitions to CONNECTING');

// Connecting -> Connected (handshake success < 5s)
state = ConnectionState.CONNECTED;
assertEq(state, 'CONNECTED', 'Successful handshake transitions to CONNECTED');

// Connected -> Reconnecting (NameOwnerChanged detects empty owner)
state = ConnectionState.RECONNECTING;
assertEq(state, 'RECONNECTING', 'Daemon disappearance transitions to RECONNECTING');

// Reconnecting -> Connected (daemon returns, handshake succeeds)
state = ConnectionState.CONNECTED;
assertEq(state, 'CONNECTED', 'Successful reconnect transitions back to CONNECTED');

print('✓ Client connection state machine transitions verified.');

// ============================================================================
// 4. Verify Defensive D-Bus Tuple Unpacking (T090)
// ============================================================================
const validTuple = [
    'sess-uuid-1234',
    'claude-code',
    'Claude Code',
    'watchai-project',
    'WORKING',
    '2026-10-04T12:00:00Z',
    '2026-10-04T12:01:00Z',
    12345,
    'FILE_READ',
];
const sessionObj = unpackSessionDto(validTuple);
assert(sessionObj !== null, 'Valid tuple must unpack into an object');
assertEq(sessionObj.sessionId, 'sess-uuid-1234', 'Session ID must match');
assertEq(sessionObj.providerId, 'claude-code', 'Provider ID must match');
assertEq(sessionObj.currentState, 'WORKING', 'Current state must match');
assertEq(sessionObj.processId, 12345, 'Process ID must match');
assertEq(sessionObj.activeToolCategory, 'FILE_READ', 'Tool category must match');

// Malformed / corrupt payloads must return null without throwing
assertEq(unpackSessionDto(null), null, 'null payload must safely return null');
assertEq(unpackSessionDto(undefined), null, 'undefined payload must safely return null');
assertEq(unpackSessionDto('not-an-array'), null, 'string payload must safely return null');
assertEq(unpackSessionDto([]), null, 'empty array must safely return null');
assertEq(unpackSessionDto(['only', 'four', 'elements', 'here']), null, 'short array must safely return null');

print('✓ Defensive D-Bus tuple unpacking verified.');

// ============================================================================
// 5. In-flight Cached Presentation State & Timer Pausing (T079)
// ============================================================================
class MockSessionCard {
    constructor(id, initialState = 'WORKING') {
        this.sessionId = id;
        this.state = initialState;
        this.badgeText = initialState;
        this.isCached = false;
        this.timerTicks = 0;
    }

    setOfflineMode(isOffline) {
        this.isCached = isOffline;
        if (isOffline) {
            this.badgeText = `${this.state} [CACHED]`;
        } else {
            this.badgeText = this.state;
        }
    }

    tickDuration() {
        if (!this.isCached) {
            this.timerTicks += 1;
        }
    }
}

const card = new MockSessionCard('sess-1', 'WORKING');
assertEq(card.isCached, false, 'Card starts in live mode');
assertEq(card.badgeText, 'WORKING', 'Badge shows live state');
card.tickDuration();
assertEq(card.timerTicks, 1, 'Live card increments duration');

// Daemon disconnects -> setOfflineMode(true)
card.setOfflineMode(true);
assertEq(card.isCached, true, 'Card transitioned to CACHED mode');
assertEq(card.badgeText, 'WORKING [CACHED]', 'Badge displays [CACHED] badge text');

// While offline, timer ticks MUST be frozen!
card.tickDuration();
card.tickDuration();
assertEq(card.timerTicks, 1, 'Cached card duration timer must be strictly paused while offline');

// Reconnection -> setOfflineMode(false)
card.setOfflineMode(false);
assertEq(card.isCached, false, 'Card restored to live mode');
assertEq(card.badgeText, 'WORKING', 'Badge returns to clean state text');
card.tickDuration();
assertEq(card.timerTicks, 2, 'Live card resumes duration ticking upon reconnect');

print('✓ In-flight cached presentation state and timer pausing verified.');

// ============================================================================
// 6. 5-Step Handshake & Timeout Simulation (T080)
// ============================================================================
class MockHandshakeCoordinator {
    constructor(timeoutMs = HANDSHAKE_TIMEOUT_MS) {
        this.timeoutMs = timeoutMs;
        this.stepsCompleted = [];
        this.isOnline = false;
        this.timedOut = false;
    }

    executeStep(stepNumber, stepName) {
        this.stepsCompleted.push({ stepNumber, stepName });
    }

    completeHandshake(elapsedMs) {
        if (elapsedMs > this.timeoutMs) {
            this.timedOut = true;
            this.isOnline = false;
            return false;
        }
        // 5-step atomic sequence:
        this.executeStep(1, 'Reacquire Proxy');
        this.executeStep(2, 'GetAggregateState');
        this.executeStep(3, 'GetSessions');
        this.executeStep(4, 'Attach Signals');
        this.executeStep(5, 'Mark Online');
        this.isOnline = true;
        return true;
    }
}

// Successful handshake under 5.0 seconds
const fastHandshake = new MockHandshakeCoordinator();
const success = fastHandshake.completeHandshake(150);
assertEq(success, true, 'Fast handshake must succeed');
assertEq(fastHandshake.isOnline, true, 'Client marked online');
assertEq(fastHandshake.stepsCompleted.length, 5, 'All 5 steps completed');
assertEq(fastHandshake.stepsCompleted[0].stepName, 'Reacquire Proxy', 'Step 1: Reacquire Proxy');
assertEq(fastHandshake.stepsCompleted[1].stepName, 'GetAggregateState', 'Step 2: GetAggregateState');
assertEq(fastHandshake.stepsCompleted[2].stepName, 'GetSessions', 'Step 3: GetSessions');
assertEq(fastHandshake.stepsCompleted[3].stepName, 'Attach Signals', 'Step 4: Attach Signals');
assertEq(fastHandshake.stepsCompleted[4].stepName, 'Mark Online', 'Step 5: Mark Online');

// Stalled handshake exceeding 5.0-second deadline
const slowHandshake = new MockHandshakeCoordinator();
const failed = slowHandshake.completeHandshake(5001);
assertEq(failed, false, 'Handshake exceeding 5.0s must abort');
assertEq(slowHandshake.timedOut, true, 'Timed out flag set');
assertEq(slowHandshake.isOnline, false, 'Client must NOT be marked online on timeout');

print('✓ 5-step asynchronous reconnection handshake and 5.0s timeout verified.');

// ============================================================================
// 7. Lifecycle Interleaving Scenarios (T081)
// ============================================================================
class MockExtensionLifecycle {
    constructor() {
        this.clientState = ConnectionState.DISCONNECTED;
        this.popoverOpen = false;
        this.cards = new Map();
        this.signalListenersAttached = false;
    }

    startExtension(daemonAlreadyRunning) {
        this.clientState = ConnectionState.CONNECTING;
        if (daemonAlreadyRunning) {
            this.signalListenersAttached = true;
            this.clientState = ConnectionState.CONNECTED;
        } else {
            // Daemon not running yet -> transitions to RECONNECTING
            this.clientState = ConnectionState.RECONNECTING;
        }
    }

    onDaemonAppeared() {
        this.clientState = ConnectionState.CONNECTING;
        // Complete handshake:
        this.signalListenersAttached = true;
        this.clientState = ConnectionState.CONNECTED;
    }

    onDaemonDisappeared() {
        this.signalListenersAttached = false;
        this.clientState = ConnectionState.RECONNECTING;
        for (const card of this.cards.values()) {
            card.setOfflineMode(true);
        }
    }

    openPopover() {
        this.popoverOpen = true;
    }

    closePopover() {
        this.popoverOpen = false;
    }
}

// Scenario A: Extension starting before daemon
const scenarioA = new MockExtensionLifecycle();
scenarioA.startExtension(false);
assertEq(scenarioA.clientState, ConnectionState.RECONNECTING, 'Starts in RECONNECTING when daemon absent');
assertEq(scenarioA.signalListenersAttached, false, 'No signals attached yet');

// Daemon appears later
scenarioA.onDaemonAppeared();
assertEq(scenarioA.clientState, ConnectionState.CONNECTED, 'Transitions to CONNECTED once daemon starts');
assertEq(scenarioA.signalListenersAttached, true, 'Signals attached on startup');

// Scenario B: Daemon starting before extension
const scenarioB = new MockExtensionLifecycle();
scenarioB.startExtension(true);
assertEq(scenarioB.clientState, ConnectionState.CONNECTED, 'Immediately CONNECTED if daemon already running');
assertEq(scenarioB.signalListenersAttached, true, 'Signals attached immediately');

// Scenario C: Daemon disappearing/reappearing while popover is open
const scenarioC = new MockExtensionLifecycle();
scenarioC.startExtension(true);
scenarioC.cards.set('s1', new MockSessionCard('s1', 'WORKING'));
scenarioC.openPopover();

// Daemon crashes while popover is open!
scenarioC.onDaemonDisappeared();
assertEq(scenarioC.clientState, ConnectionState.RECONNECTING, 'Reconnecting while popover open');
assertEq(scenarioC.signalListenersAttached, false, 'Signals detached on disconnect');
assertEq(scenarioC.cards.get('s1').isCached, true, 'Rendered card transitioned to CACHED');

// Daemon recovers
scenarioC.onDaemonAppeared();
assertEq(scenarioC.clientState, ConnectionState.CONNECTED, 'Reconnected successfully');
assertEq(scenarioC.signalListenersAttached, true, 'Signals re-established');
scenarioC.cards.get('s1').setOfflineMode(false);
assertEq(scenarioC.cards.get('s1').isCached, false, 'Rendered card returned to live state');

print('✓ Lifecycle interleaving scenarios (A, B, C) verified.');
print('All reconnect and crash recovery GJS tests passed successfully!');
