import {
    activateWindowForProcess,
    computeBackoffDelay,
    extractProjectFromTitle,
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

// 1. extractProjectFromTitle unit tests
{
    assert(extractProjectFromTitle('◐ watchai') === 'watchai', 'Should strip status runes');
    assert(extractProjectFromTitle('✳ anbar') === 'anbar', 'Should strip status runes');
    assert(extractProjectFromTitle('hossein@ubuntu:~/Projects/WatchAI') === 'watchai', 'Should isolate leaf dir from prompt');
    assert(extractProjectFromTitle('hossein@ubuntu:~/Projects/Anbar') === 'anbar', 'Should isolate leaf dir from prompt');
    assert(extractProjectFromTitle('hossein@ubuntu:~/Projects') === 'projects', 'Should extract projects from prompt');
    assert(extractProjectFromTitle('Projects') === 'projects', 'Should extract plain title');
    assert(extractProjectFromTitle('/home/user/work/my-app/') === 'my-app', 'Should extract trailing slash path');
    assert(extractProjectFromTitle('') === '', 'Empty string returns empty');
    assert(extractProjectFromTitle(null) === '', 'Null returns empty');
    console.log('✓ extractProjectFromTitle parses project names cleanly across prompt styles.');
}

// 2. findTerminalTabMatch input edge cases
{
    assert(findTerminalTabMatch(null) === null, 'null project name should return null');
    assert(findTerminalTabMatch('') === null, 'empty project name should return null');
    assert(findTerminalTabMatch('   ') === null, 'whitespace project name should return null');
    assert(findTerminalTabMatch(123) === null, 'non-string project name should return null');
    console.log('✓ findTerminalTabMatch rejects invalid inputs safely.');
}

// 3. Mock AT-SPI tree for multi-tab matching verification
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

// Window 0: Multi-tab terminal with WatchAI, Anbar, and generic desktop
const tabList1 = new MockTabList([
    new MockTab('hossein@ubuntu:~/Desktop'),
    new MockTab('◑ watchai'),
    new MockTab('hossein@ubuntu:~/Projects/WatchAI'),
    new MockTab('✳ anbar'),
]);
const win1 = new MockWindow('hossein@ubuntu:~/Desktop', tabList1);

// Window 1: Standalone terminal window titled "Projects" (e.g. for Codex)
const tabList2 = new MockTabList([
    new MockTab(''),
]);
const win2 = new MockWindow('Projects', tabList2);

const termApp = new MockApp('gnome-terminal-server', 60200, [win1, win2]);

const mockDesktop = {
    get_child_count() { return 1; },
    get_child_at_index(idx) { return idx === 0 ? termApp : null; },
};

{
    // Test WatchAI match
    const matchWatchAI = findTerminalTabMatch('WatchAI', new Set([60200]), mockDesktop);
    assert(matchWatchAI !== null, 'Should find matching tab for WatchAI');
    assert(matchWatchAI.tabIndex === 1, `Expected tab index 1 (◑ watchai), got ${matchWatchAI.tabIndex}`);
    assert(matchWatchAI.winName === 'hossein@ubuntu:~/desktop', 'Expected win1 match');

    // Test Anbar match
    const matchAnbar = findTerminalTabMatch('/home/hossein/Projects/Anbar', new Set([60200]), mockDesktop);
    assert(matchAnbar !== null, 'Should find matching tab for Anbar from full path');
    assert(matchAnbar.tabIndex === 3, `Expected tab index 3 (✳ anbar), got ${matchAnbar.tabIndex}`);

    // Critical test: Projects (Codex) must match win2 ("Projects"), NOT win1's "~/Projects/WatchAI" tab!
    const matchProjects = findTerminalTabMatch('Projects', new Set([60200]), mockDesktop);
    assert(matchProjects !== null, 'Should find match for Projects');
    assert(matchProjects.winName === 'projects', `Expected win2 (projects), got ${matchProjects.winName}`);
    assert(matchProjects.tabIndex === 0, `Expected tab index 0, got ${matchProjects.tabIndex}`);

    console.log('✓ AT-SPI tab list matching prioritizes active agent tabs and isolates parent directories.');
}

// 4. Mock activateWindowForProcess window scoring and tab switching
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

    // Session for Codex running in "Projects" under gnome-terminal-server
    const codexSession = {
        processId: 575764,
        projectName: 'Projects',
    };

    const result = activateWindowForProcess(codexSession, '', mockDesktop);
    assert(result === true, 'activateWindowForProcess should succeed for Codex');
    assert(winB.activated === true, 'winB ("Projects") should be activated for Codex session');
    assert(winB.get_workspace().activated === true, 'winB workspace should be activated');
    assert(globalThis.Main.activatedWindow === winB, 'Main.activateWindow should target winB');

    console.log('✓ activateWindowForProcess correctly targets the dedicated Projects window for Codex.');
}

console.log('\nAll utils, extraction, and tab-switching GJS tests passed successfully!');
