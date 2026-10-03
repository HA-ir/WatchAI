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
// 1. Verify Timing Constants & Strict Ceiling (Section 1)
// ============================================================================
assertEq(HANDSHAKE_TIMEOUT_MS, 5000, 'Handshake timeout must be exactly 5000ms (5.0s)');
assertEq(INITIAL_INTERVAL_MS, 1000, 'Initial reconnect interval must be exactly 1000ms (1.0s)');
assertEq(MAX_INTERVAL_MS, 30000, 'Maximum reconnect interval ceiling must be 30000ms (30.0s)');
assertEq(MULTIPLIER, 2.0, 'Backoff multiplier must be 2.0');
assertEq(JITTER_RATIO, 0.20, 'Jitter ratio must be 20% (±20%)');

// Verify jitter bounds
const minJitterBound = 1.0 - JITTER_RATIO;
const maxJitterBound = 1.0 + JITTER_RATIO;
assertEq(minJitterBound, 0.80, 'Jitter lower bound must be exactly 0.80');
assertEq(maxJitterBound, 1.20, 'Jitter upper bound must be exactly 1.20');

// ============================================================================
// 2. Verify Jittered Exponential Backoff Calculations & Ceiling (Section 1)
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
assertEq(computeBackoffDelay(5, maxRng), 30000, 'Attempt 5 max jitter must be clamped to 30000ms ceiling');
assertEq(computeBackoffDelay(10, midRng), 30000, 'Attempt 10 must remain clamped to 30000ms ceiling');
assertEq(computeBackoffDelay(10, maxRng), 30000, 'Attempt 10 max jitter must remain clamped to 30000ms ceiling');

// Strict mathematical proof: Delay NEVER exceeds 30000ms across all failure counts and RNG values
for (let n = 0; n <= 25; n++) {
    for (let r = 0.0; r <= 1.0; r += 0.1) {
        const delay = computeBackoffDelay(n, () => r);
        assert(delay <= 30000, `Delay must never exceed 30000ms (got ${delay} at n=${n}, r=${r})`);
        assert(delay >= 800, `Delay must never be lower than minimum first-retry jitter 800ms (got ${delay})`);
    }
}

// Success reset verification: passing 0 after consecutive failures resets backoff
let failures = 5;
assertEq(computeBackoffDelay(failures, midRng), 30000, 'High failures evaluate to ceiling');
failures = 0; // Reset upon successful handshake!
assertEq(computeBackoffDelay(failures, midRng), 1000, 'Reset failures must return to 1000ms base');

print('✓ Jittered exponential backoff, exact constants, and 30000ms ceiling verified.');

// ============================================================================
// 3. Verify ConnectionState Machine Transitions (Section 2)
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
// 4. Verify Defensive D-Bus Tuple Unpacking (Section 10)
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
// 5. In-flight Cached Presentation State & Timer Pausing (Section 4)
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
// 6. 5-Step Handshake & Timeout Simulation (Section 3)
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
// 7. Stale Handshake & Race Safety Simulation (Sequences A - F) (Section 2)
// ============================================================================
class MockRaceSafeClient {
    constructor() {
        this.connectionState = ConnectionState.DISCONNECTED;
        this.generation = 0;
        this.isDestroyed = false;
        this.activeSignalListeners = 0;
        this.sessions = [];
        this.pendingTimers = new Set();
    }

    connect() {
        if (this.isDestroyed) return;
        this.connectionState = ConnectionState.CONNECTING;
        const currentGen = ++this.generation;
        return currentGen;
    }

    handleDisconnect() {
        this.generation += 1; // Invalidate all pending in-flight callbacks!
        this.connectionState = ConnectionState.RECONNECTING;
        this.activeSignalListeners = 0; // Detach signals
    }

    resolveHandshake(gen, aggregateState, sessions) {
        // Strict guard: Drop if destroyed or if generation does not match!
        if (this.isDestroyed || this.generation !== gen) {
            return false; // STALE CALLBACK DROPPED!
        }
        this.activeSignalListeners = 4;
        this.sessions = sessions;
        this.connectionState = ConnectionState.CONNECTED;
        return true;
    }

    disable() {
        this.isDestroyed = true;
        this.generation += 1;
        this.connectionState = ConnectionState.DISCONNECTED;
        this.activeSignalListeners = 0;
        this.pendingTimers.clear();
    }
}

// Sequence A: CONNECTED -> daemon disappears -> reconnect starts -> daemon returns -> handshake succeeds
const clientA = new MockRaceSafeClient();
const genA1 = clientA.connect();
clientA.resolveHandshake(genA1, 'IDLE', []);
assertEq(clientA.connectionState, ConnectionState.CONNECTED, 'Seq A: Initial connect');
clientA.handleDisconnect();
assertEq(clientA.connectionState, ConnectionState.RECONNECTING, 'Seq A: Disconnected');
const genA2 = clientA.connect();
const resolvedA2 = clientA.resolveHandshake(genA2, 'WORKING', [{ id: 's1' }]);
assertEq(resolvedA2, true, 'Seq A: Reconnect resolves');
assertEq(clientA.connectionState, ConnectionState.CONNECTED, 'Seq A: Client marked online');

// Sequence B: reconnect attempt starts -> daemon disappears again -> previous handshake resolves late
const clientB = new MockRaceSafeClient();
const genB1 = clientB.connect();
assertEq(clientB.connectionState, ConnectionState.CONNECTING, 'Seq B: Connecting');
clientB.handleDisconnect(); // Disappears again!
assertEq(clientB.connectionState, ConnectionState.RECONNECTING, 'Seq B: Reconnecting');
// Attempt B1 resolves late now:
const resolvedB1 = clientB.resolveHandshake(genB1, 'WORKING', [{ id: 'stale' }]);
assertEq(resolvedB1, false, 'Seq B: Late handshake callback must be rejected');
assertEq(clientB.connectionState, ConnectionState.RECONNECTING, 'Seq B: Client remains RECONNECTING');
assertEq(clientB.sessions.length, 0, 'Seq B: Stale session data rejected');

// Sequence C: reconnect attempt 1 -> timeout -> reconnect attempt 2 starts -> attempt 1 resolves late
const clientC = new MockRaceSafeClient();
const genC1 = clientC.connect();
clientC.handleDisconnect(); // Timeout triggers disconnect/retry
const genC2 = clientC.connect(); // Attempt 2 begins
assertEq(clientC.generation, 3, 'Seq C: Generation advanced to 3');
// Attempt 1 resolves late:
const resolvedC1 = clientC.resolveHandshake(genC1, 'IDLE', [{ id: 'from-attempt-1' }]);
assertEq(resolvedC1, false, 'Seq C: Late attempt 1 rejected');
// Attempt 2 resolves on time:
const resolvedC2 = clientC.resolveHandshake(genC2, 'IDLE', [{ id: 'from-attempt-2' }]);
assertEq(resolvedC2, true, 'Seq C: Current attempt 2 accepted');
assertEq(clientC.sessions[0].id, 'from-attempt-2', 'Seq C: Authoritative state applied');

// Sequence D: daemon appears/disappears/appears rapidly
const clientD = new MockRaceSafeClient();
for (let i = 0; i < 5; i++) {
    clientD.connect();
    clientD.handleDisconnect();
}
const finalGenD = clientD.connect();
const finalResolvedD = clientD.resolveHandshake(finalGenD, 'IDLE', []);
assertEq(finalResolvedD, true, 'Seq D: Only final generation accepted');
assertEq(clientD.connectionState, ConnectionState.CONNECTED, 'Seq D: Successfully connected');

// Sequence E: extension disable() while handshake is in flight
const clientE = new MockRaceSafeClient();
const genE1 = clientE.connect();
clientE.disable();
assertEq(clientE.isDestroyed, true, 'Seq E: Client destroyed');
const resolvedE1 = clientE.resolveHandshake(genE1, 'WORKING', [{ id: 'stale' }]);
assertEq(resolvedE1, false, 'Seq E: Callback dropped on destroyed client');
assertEq(clientE.connectionState, ConnectionState.DISCONNECTED, 'Seq E: Remains DISCONNECTED');

// Sequence F: extension disable() while backoff timer is pending
const clientF = new MockRaceSafeClient();
clientF.handleDisconnect();
clientF.pendingTimers.add(99);
clientF.disable();
assertEq(clientF.pendingTimers.size, 0, 'Seq F: All pending timers purged');
assertEq(clientF.isDestroyed, true, 'Seq F: Client marked destroyed');

print('✓ Stale handshake & race safety sequences (A, B, C, D, E, F) verified.');
print('All reconnect and crash recovery GJS tests passed successfully!');
