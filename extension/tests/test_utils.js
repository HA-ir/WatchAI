import {
    activateWindowForProcess,
    computeBackoffDelay,
    findTerminalTabMatch,
    formatDuration,
    getStatePriority,
    sanitizeProjectName,
    unpackSessionDto,
} from '../utils.js';

function assert(condition, message) {
    if (!condition) {
        throw new Error(message || 'Assertion failed');
    }
}

// 1. findTerminalTabMatch input edge cases
{
    assert(findTerminalTabMatch(null) === null, 'null project name should return null');
    assert(findTerminalTabMatch('') === null, 'empty project name should return null');
    assert(findTerminalTabMatch('   ') === null, 'whitespace project name should return null');
    assert(findTerminalTabMatch(123) === null, 'non-string project name should return null');
    console.log('✓ findTerminalTabMatch rejects invalid inputs safely.');
}

// 2. Mock AT-SPI tree for multi-tab matching verification
{
    class MockTab {
        constructor(name) {
            this._name = name;
        }
        get_name() { return this._name; }
        get_role_name() { return 'page tab'; }
        get_child_count() { return 0; }
        get_child_at_index() { return null; }
    }

    class MockTabList {
        constructor(tabs) {
            this._tabs = tabs;
            this.selectedChild = -1;
        }
        get_role_name() { return 'page tab list'; }
        get_child_count() { return this._tabs.length; }
        get_child_at_index(idx) { return this._tabs[idx]; }
        select_child(idx) {
            this.selectedChild = idx;
            return true;
        }
    }

    class MockWindow {
        constructor(name, tabList) {
            this._name = name;
            this._tabList = tabList;
        }
        get_name() { return this._name; }
        get_role_name() { return 'frame'; }
        get_child_count() { return this._tabList ? 1 : 0; }
        get_child_at_index(idx) { return idx === 0 ? this._tabList : null; }
    }

    class MockApp {
        constructor(name, pid, windows) {
            this._name = name;
            this._pid = pid;
            this._windows = windows;
        }
        get_name() { return this._name; }
        get_role_name() { return 'application'; }
        get_process_id() { return this._pid; }
        get_child_count() { return this._windows.length; }
        get_child_at_index(idx) { return this._windows[idx]; }
    }

    const tabList1 = new MockTabList([
        new MockTab('hossein@ubuntu:~/Desktop'),
        new MockTab('◑ watchai'),
        new MockTab('hossein@ubuntu:~/Projects/WatchAI'),
        new MockTab('✳ anbar'),
    ]);
    const win1 = new MockWindow('hossein@ubuntu:~/Desktop', tabList1);
    const termApp = new MockApp('gnome-terminal-server', 60200, [win1]);

    const mockDesktop = {
        get_child_count() { return 1; },
        get_child_at_index(idx) { return idx === 0 ? termApp : null; },
    };

    const matchWatchAI = findTerminalTabMatch('WatchAI', new Set([60200]), mockDesktop);
    assert(matchWatchAI !== null, 'Should find matching tab for WatchAI');
    assert(matchWatchAI.tabIndex === 1, `Expected tab index 1 (◑ watchai), got ${matchWatchAI.tabIndex}`);
    assert(matchWatchAI.winName === 'hossein@ubuntu:~/desktop', 'Expected window name match');

    // Test tab selection
    matchWatchAI.tabList.select_child(matchWatchAI.tabIndex);
    assert(tabList1.selectedChild === 1, 'MockTabList should record selected child 1');

    // Test Anbar match
    const matchAnbar = findTerminalTabMatch('/home/hossein/Projects/Anbar', new Set([60200]), mockDesktop);
    assert(matchAnbar !== null, 'Should find matching tab for Anbar from full path');
    assert(matchAnbar.tabIndex === 3, `Expected tab index 3 (✳ anbar), got ${matchAnbar.tabIndex}`);

    matchAnbar.tabList.select_child(matchAnbar.tabIndex);
    assert(tabList1.selectedChild === 3, 'MockTabList should record selected child 3');

    console.log('✓ AT-SPI tab list matching prioritizes active agent tab with symbols.');
}

// 3. Mock activateWindowForProcess window scoring and tab switching
{
    class MockMetaWindow {
        constructor(pid, title, wmClass) {
            this._pid = pid;
            this._title = title;
            this._wmClass = wmClass;
            this.activated = false;
            this.unminimized = false;
            this.minimized = false;
            this._workspace = {
                activated: false,
                activate: () => { this._workspace.activated = true; },
            };
        }
        get_pid() { return this._pid; }
        get_title() { return this._title; }
        get_wm_class() { return this._wmClass; }
        get_workspace() { return this._workspace; }
        activate() { this.activated = true; }
        unminimize() { this.unminimized = true; }
    }

    class MockWindowActor {
        constructor(metaWindow) {
            this._metaWindow = metaWindow;
        }
        get_meta_window() { return this._metaWindow; }
    }

    const winA = new MockMetaWindow(60200, 'hossein@ubuntu:~/Desktop', 'Gnome-terminal');
    const winB = new MockMetaWindow(60200, 'Projects', 'Gnome-terminal');

    globalThis.global = {
        get_window_actors: () => [new MockWindowActor(winA), new MockWindowActor(winB)],
        get_current_time: () => 1000,
    };
    globalThis.Main = {
        activatedWindow: null,
        activateWindow: (win) => { globalThis.Main.activatedWindow = win; },
    };

    // Session for Anbar running under gnome-terminal-server (PID 60200)
    const session = {
        processId: 165238, // Claude process PID
        projectName: 'Anbar',
    };

    // winA houses the tab for Anbar!
    const result = activateWindowForProcess(session);
    assert(result === true, 'activateWindowForProcess should succeed');
    assert(winA.activated === true, 'winA should be activated because it houses Anbar tab');
    assert(winA.get_workspace().activated === true, 'winA workspace should be activated');
    assert(globalThis.Main.activatedWindow === winA, 'Main.activateWindow should target winA');

    console.log('✓ activateWindowForProcess selects and raises terminal window containing target tab.');
}

console.log('\nAll utils and tab-switching GJS tests passed successfully!');
