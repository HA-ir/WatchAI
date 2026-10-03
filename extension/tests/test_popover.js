// GJS test for WatchAI Popover logic: duration formatting, priority sorting, and state handling
import { formatDuration, getStatePriority } from '../utils.js';

function testFormatDuration() {
    // Test null / invalid input
    if (formatDuration(null) !== '00:00') {
        throw new Error('formatDuration(null) should return 00:00');
    }
    if (formatDuration('') !== '00:00') {
        throw new Error('formatDuration("") should return 00:00');
    }
    print('✓ formatDuration handles null/empty safely.');
}

function testGetStatePriority() {
    const errorPrio = getStatePriority('ERROR');
    const waitingPrio = getStatePriority('WAITING');
    const workingPrio = getStatePriority('WORKING');
    const startingPrio = getStatePriority('STARTING');
    const cancelledPrio = getStatePriority('CANCELLED');
    const successPrio = getStatePriority('SUCCESS');
    const unknownPrio = getStatePriority('UNKNOWN');
    const idlePrio = getStatePriority('IDLE');

    if (errorPrio <= waitingPrio) {
        throw new Error('ERROR priority must be higher than WAITING');
    }
    if (waitingPrio <= workingPrio) {
        throw new Error('WAITING priority must be higher than WORKING');
    }
    if (workingPrio <= startingPrio) {
        throw new Error('WORKING priority must be higher than STARTING');
    }
    if (startingPrio <= cancelledPrio) {
        throw new Error('STARTING priority must be higher than CANCELLED');
    }
    if (cancelledPrio <= successPrio) {
        throw new Error('CANCELLED priority must be higher than SUCCESS');
    }
    if (successPrio <= unknownPrio) {
        throw new Error('SUCCESS priority must be higher than UNKNOWN');
    }
    if (unknownPrio <= idlePrio) {
        throw new Error('UNKNOWN priority must be higher than IDLE');
    }

    print('✓ State priority values match mathematical hierarchy.');
}

function testCardSortingLogic() {
    const mockSessions = [
        { sessionId: 's1', currentState: 'WORKING', startedAt: '2026-10-03T10:00:00Z' },
        { sessionId: 's2', currentState: 'WAITING', startedAt: '2026-10-03T10:01:00Z' },
        { sessionId: 's3', currentState: 'IDLE', startedAt: '2026-10-03T09:50:00Z' },
        { sessionId: 's4', currentState: 'ERROR', startedAt: '2026-10-03T10:02:00Z' },
    ];

    const sorted = [...mockSessions].sort((a, b) => {
        const prioA = getStatePriority(a.currentState);
        const prioB = getStatePriority(b.currentState);
        if (prioA !== prioB) {
            return prioB - prioA;
        }
        return b.startedAt.localeCompare(a.startedAt);
    });

    // Expect ERROR first, then WAITING, then WORKING, then IDLE
    if (sorted[0].sessionId !== 's4' || sorted[0].currentState !== 'ERROR') {
        throw new Error('ERROR session should be first in sorted order');
    }
    if (sorted[1].sessionId !== 's2' || sorted[1].currentState !== 'WAITING') {
        throw new Error('WAITING session should be second in sorted order');
    }
    if (sorted[2].sessionId !== 's1' || sorted[2].currentState !== 'WORKING') {
        throw new Error('WORKING session should be third in sorted order');
    }
    if (sorted[3].sessionId !== 's3' || sorted[3].currentState !== 'IDLE') {
        throw new Error('IDLE session should be last in sorted order');
    }

    print('✓ Card sorting logic correctly elevates urgent states (ERROR, WAITING) to the top.');
}

try {
    testFormatDuration();
    testGetStatePriority();
    testCardSortingLogic();
    print('All popover GJS tests passed successfully!');
} catch (e) {
    printerr('Test failed: ' + e);
    imports.system.exit(1);
}
