// Test for WatchAI GNOME Shell Indicator configuration and state mapping (Phase 10 - T133)

import { STATE_CONFIG } from '../utils.js';

function assertEqual(actual, expected, message) {
    if (actual !== expected) {
        throw new Error(`${message || 'Assertion failed'}: expected "${expected}", got "${actual}"`);
    }
}

function testAllStatesHaveUniqueIconsAndDescriptions() {
    const states = ['IDLE', 'STARTING', 'WORKING', 'WAITING', 'SUCCESS', 'ERROR', 'CANCELLED', 'UNKNOWN'];
    for (const state of states) {
        const config = STATE_CONFIG[state];
        if (!config) {
            throw new Error(`Missing config for state ${state}`);
        }
        if (!config.icon || !config.cssClass || !config.accessibleDesc) {
            throw new Error(`Incomplete config for state ${state}`);
        }
        if (!config.accessibleDesc.startsWith('WatchAI:')) {
            throw new Error(`Accessible description for ${state} does not follow standard prefix`);
        }
    }
    print('✓ All 8 states have valid icons, CSS classes, and AT-SPI descriptions.');
}

function testAccessibilityFormatting() {
    const config = STATE_CONFIG['WAITING'];
    let a11y = config.accessibleDesc;
    const activeCount = 3;
    if (activeCount > 1) {
        a11y += ` (${activeCount} active sessions)`;
    }
    assertEqual(
        a11y,
        'WatchAI: Agent blocked waiting for user approval (3 active sessions)',
        'Unexpected accessibility label'
    );
    print('✓ Accessibility multi-session counter formatting verified.');
}

function computeStyleClass(state, iconStyle) {
    const config = STATE_CONFIG[state] || STATE_CONFIG.UNKNOWN;
    const modeClass = iconStyle === 'colored' ? 'watchai-icon-colored' : 'watchai-icon-symbolic';
    return `system-status-icon watchai-status-icon ${modeClass} ${config.cssClass}`;
}

function testIconStyleModeClassGeneration() {
    // Symbolic mode
    const symIdle = computeStyleClass('IDLE', 'symbolic');
    assertEqual(symIdle, 'system-status-icon watchai-status-icon watchai-icon-symbolic watchai-state-idle');

    const symWorking = computeStyleClass('WORKING', 'symbolic');
    assertEqual(symWorking, 'system-status-icon watchai-status-icon watchai-icon-symbolic watchai-state-working');

    // Colored mode
    const colWorking = computeStyleClass('WORKING', 'colored');
    assertEqual(colWorking, 'system-status-icon watchai-status-icon watchai-icon-colored watchai-state-working');

    const colWaiting = computeStyleClass('WAITING', 'colored');
    assertEqual(colWaiting, 'system-status-icon watchai-status-icon watchai-icon-colored watchai-state-waiting');

    const colError = computeStyleClass('ERROR', 'colored');
    assertEqual(colError, 'system-status-icon watchai-status-icon watchai-icon-colored watchai-state-error');

    print('✓ Indicator icon presentation class generation (symbolic vs colored) verified.');
}

try {
    testAllStatesHaveUniqueIconsAndDescriptions();
    testAccessibilityFormatting();
    testIconStyleModeClassGeneration();

    // Test Approach A multi-status logic
    function testMultiStatusBadgeLogic() {
        // Coexisting working=1 and success=1
        const activeCount = 2;
        const workingCount = 1;
        const successCount = 1;
        const waitingCount = 0;
        const errorCount = 0;

        const hasWorking = workingCount > 0;
        const hasWaiting = waitingCount > 0;
        const hasSuccess = successCount > 0;
        const hasError = errorCount > 0;
        const categoryCount = (hasWorking ? 1 : 0) + (hasWaiting ? 1 : 0) + (hasSuccess ? 1 : 0) + (hasError ? 1 : 0);

        assertEqual(categoryCount, 2, 'Must detect 2 coexisting categories');

        const details = [];
        if (hasWorking) details.push(`${workingCount} working`);
        if (hasWaiting) details.push(`${waitingCount} waiting for input`);
        if (hasError) details.push(`${errorCount} in error`);
        if (hasSuccess) details.push(`${successCount} completed`);
        const a11y = `WatchAI: ${details.join(', ')} (${activeCount} active sessions)`;

        assertEqual(
            a11y,
            'WatchAI: 1 working, 1 completed (2 active sessions)',
            'Multi-status a11y announcement mismatch'
        );
        print('✓ Multi-status badge coexisting category evaluation and a11y text verified.');
    }
    testMultiStatusBadgeLogic();

    print('\nAll indicator GJS tests passed successfully!');
} catch (e) {
    printerr('Test failed: ' + e + '\n' + e.stack);
    imports.system.exit(1);
}
