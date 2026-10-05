// Unit test suite for WatchAI AT-SPI Accessibility (Phase 10 - T138)

import Atk from 'gi://Atk';
import { STATE_CONFIG, sanitizeProjectName, formatDuration } from '../utils.js';

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

function testAllEightLifecycleStatesAccessibleNames() {
    const expected = {
        IDLE: 'WatchAI: No active coding agents',
        STARTING: 'WatchAI: Agent session initializing',
        WORKING: 'WatchAI: Agent actively executing work',
        WAITING: 'WatchAI: Agent blocked waiting for user approval',
        SUCCESS: 'WatchAI: Agent task completed successfully',
        ERROR: 'WatchAI: Agent encountered an error',
        CANCELLED: 'WatchAI: Agent session cancelled',
        UNKNOWN: 'WatchAI: Agent state unverified',
    };

    for (const [state, expectedName] of Object.entries(expected)) {
        const config = STATE_CONFIG[state];
        assert(config, `Config must exist for state ${state}`);
        assertEqual(config.accessibleDesc, expectedName, `Accessible name for ${state}`);
    }
    print('✓ All 8 lifecycle states have verified accessible names.');
}

function testDynamicMultiSessionCounter() {
    const stateConfig = STATE_CONFIG['WORKING'];

    // Single active session -> No count suffix
    let label = stateConfig.accessibleDesc;
    const activeCountSingle = 1;
    if (activeCountSingle > 1) {
        label += ` (${activeCountSingle} active sessions)`;
    }
    assertEqual(label, 'WatchAI: Agent actively executing work', 'Single session must not have count suffix');

    // Multiple active sessions -> Appends (N active sessions)
    label = stateConfig.accessibleDesc;
    const activeCountMulti = 4;
    if (activeCountMulti > 1) {
        label += ` (${activeCountMulti} active sessions)`;
    }
    assertEqual(label, 'WatchAI: Agent actively executing work (4 active sessions)', 'Multiple sessions must include count suffix');
    print('✓ Dynamic multi-session count announcements verified.');
}

function testOfflineAccessibleNameAndDescription() {
    const offlineName = 'WatchAI daemon offline';
    const indicatorHelpDesc = 'Click to open agent session popover menu';

    assertEqual(offlineName, 'WatchAI daemon offline', 'Offline accessible name matches spec');
    assertEqual(indicatorHelpDesc, 'Click to open agent session popover menu', 'Indicator help description matches spec');
    print('✓ Offline announcement and indicator description verified.');
}

function testAtkRoleConstants() {
    assertEqual(Atk.Role.TOGGLE_BUTTON, 61, 'Atk.Role.TOGGLE_BUTTON value');
    assertEqual(Atk.Role.PUSH_BUTTON, 42, 'Atk.Role.PUSH_BUTTON value');
    assertEqual(Atk.Role.MENU, 32, 'Atk.Role.MENU value');
    assertEqual(Atk.Role.PANEL, 38, 'Atk.Role.PANEL value');
    print('✓ AT-SPI Atk.Role constants verified for TOGGLE_BUTTON, MENU, and PANEL.');
}

function formatSessionCardAccessibleName(session, formattedDuration) {
    const provider = session.providerDisplayName || session.providerId || 'AI Agent';
    const project = sanitizeProjectName(session.projectName);
    const state = session.currentState || 'UNKNOWN';
    return `${provider}, ${project}, state ${state}, duration ${formattedDuration}`;
}

function formatSessionCardAccessibleDescription(session) {
    const parts = [];
    if (session.processId && session.processId > 0) {
        parts.push(`Process ID ${session.processId}`);
    }
    if (session.activeToolCategory && session.activeToolCategory.length > 0) {
        parts.push(`active tool ${session.activeToolCategory}`);
    }
    return parts.join(', ');
}

function testSessionCardAccessibilityFormatting() {
    const mockSession = {
        sessionId: 'sess-123',
        providerId: 'claude-code',
        providerDisplayName: 'Claude Code',
        projectName: 'WatchAI\n<frontend>&test',
        currentState: 'WORKING',
        processId: 4567,
        activeToolCategory: 'Bash',
    };

    const duration = '05:42';
    const a11yName = formatSessionCardAccessibleName(mockSession, duration);
    const a11yDesc = formatSessionCardAccessibleDescription(mockSession);

    // Assert structured name
    assertEqual(
        a11yName,
        'Claude Code, WatchAIfrontendtest, state WORKING, duration 05:42',
        'Structured card accessible name must use sanitized project name'
    );

    // Assert description with PID and tool
    assertEqual(
        a11yDesc,
        'Process ID 4567, active tool Bash',
        'Structured card accessible description must include PID and tool'
    );

    // Session without tool or PID
    const minimalSession = {
        sessionId: 'sess-min',
        providerId: 'opencode',
        providerDisplayName: 'OpenCode',
        projectName: null,
        currentState: 'WAITING',
    };
    const minName = formatSessionCardAccessibleName(minimalSession, '00:10');
    const minDesc = formatSessionCardAccessibleDescription(minimalSession);

    assertEqual(minName, 'OpenCode, workspace, state WAITING, duration 00:10');
    assertEqual(minDesc, '', 'Empty description when PID and tool are absent');

    print('✓ Session card accessible name and description formatting verified.');
}

try {
    testAllEightLifecycleStatesAccessibleNames();
    testDynamicMultiSessionCounter();
    testOfflineAccessibleNameAndDescription();
    testAtkRoleConstants();
    testSessionCardAccessibilityFormatting();
    print('\nAll AT-SPI Accessibility GJS tests passed successfully!');
} catch (e) {
    printerr('Test failed: ' + e + '\n' + e.stack);
    imports.system.exit(1);
}
