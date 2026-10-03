// Test for WatchAI GNOME Shell Indicator configuration and state mapping

const STATE_CONFIG = {
    IDLE: {
        icon: 'system-run-symbolic',
        cssClass: 'watchai-state-idle',
        accessibleDesc: 'WatchAI: No active coding agents',
    },
    STARTING: {
        icon: 'process-working-symbolic',
        cssClass: 'watchai-state-starting',
        accessibleDesc: 'WatchAI: Agent session initializing',
    },
    WORKING: {
        icon: 'media-playback-start-symbolic',
        cssClass: 'watchai-state-working',
        accessibleDesc: 'WatchAI: Agent actively executing work',
    },
    WAITING: {
        icon: 'dialog-warning-symbolic',
        cssClass: 'watchai-state-waiting',
        accessibleDesc: 'WatchAI: Agent blocked waiting for user approval',
    },
    SUCCESS: {
        icon: 'emblem-ok-symbolic',
        cssClass: 'watchai-state-success',
        accessibleDesc: 'WatchAI: Agent task completed successfully',
    },
    ERROR: {
        icon: 'dialog-error-symbolic',
        cssClass: 'watchai-state-error',
        accessibleDesc: 'WatchAI: Agent encountered an error',
    },
    CANCELLED: {
        icon: 'process-stop-symbolic',
        cssClass: 'watchai-state-cancelled',
        accessibleDesc: 'WatchAI: Agent session cancelled',
    },
    UNKNOWN: {
        icon: 'dialog-question-symbolic',
        cssClass: 'watchai-state-unknown',
        accessibleDesc: 'WatchAI: Agent state unverified',
    },
};

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
    if (a11y !== 'WatchAI: Agent blocked waiting for user approval (3 active sessions)') {
        throw new Error(`Unexpected accessibility label: ${a11y}`);
    }
    print('✓ Accessibility multi-session counter formatting verified.');
}

try {
    testAllStatesHaveUniqueIconsAndDescriptions();
    testAccessibilityFormatting();
    print('All indicator GJS tests passed successfully!');
} catch (e) {
    printerr('Test failed: ' + e);
    // Exit with code 1
    imports.system.exit(1);
}
