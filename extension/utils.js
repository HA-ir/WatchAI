import GLib from 'gi://GLib';

let Atspi = null;
try {
    const mod = await import('gi://Atspi');
    Atspi = mod.default || mod;
} catch (_) {
    // AT-SPI typelib may not be present in minimal/headless environments;
    // degrade gracefully so core utility functions remain importable.
}

export function formatDuration(startedAtIso) {
    if (!startedAtIso) return '00:00';
    try {
        const start = GLib.DateTime.new_from_iso8601(startedAtIso, null);
        if (!start) return '00:00';
        const now = GLib.DateTime.new_now_utc();
        const diffSeconds = Math.max(0, now.to_unix() - start.to_unix());

        const hours = Math.floor(diffSeconds / 3600);
        const minutes = Math.floor((diffSeconds % 3600) / 60);
        const seconds = diffSeconds % 60;

        const pad = (n) => (n < 10 ? '0' + n : '' + n);
        if (hours > 0) {
            return `${pad(hours)}:${pad(minutes)}:${pad(seconds)}`;
        }
        return `${pad(minutes)}:${pad(seconds)}`;
    } catch {
        return '00:00';
    }
}

export function getStatePriority(state) {
    switch (state) {
        case 'ERROR': return 80;
        case 'WAITING': return 70;
        case 'WORKING': return 60;
        case 'STARTING': return 50;
        case 'CANCELLED': return 40;
        case 'SUCCESS': return 30;
        case 'UNKNOWN': return 20;
        case 'IDLE': return 10;
        default: return 0;
    }
}

export const ConnectionState = {
    DISCONNECTED: 'DISCONNECTED',
    CONNECTING: 'CONNECTING',
    CONNECTED: 'CONNECTED',
    RECONNECTING: 'RECONNECTING',
};

export const STATE_CONFIG = {
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

export const HANDSHAKE_TIMEOUT_MS = 5000;
export const INITIAL_INTERVAL_MS = 1000;
export const MAX_INTERVAL_MS = 30000;
export const MULTIPLIER = 2.0;
export const JITTER_RATIO = 0.20;

/// Compute exponential backoff delay with ±20% jitter.
/// Strictly capped at MAX_INTERVAL_MS (30,000 ms) regardless of jitter variation.
export function computeBackoffDelay(consecutiveFailures, randomFn = Math.random) {
    const base = Math.min(
        MAX_INTERVAL_MS,
        INITIAL_INTERVAL_MS * Math.pow(MULTIPLIER, consecutiveFailures)
    );
    const minJitter = 1.0 - JITTER_RATIO;
    const maxJitter = 1.0 + JITTER_RATIO;
    const factor = minJitter + (maxJitter - minJitter) * randomFn();
    return Math.min(MAX_INTERVAL_MS, Math.round(base * factor));
}

/// Defensively unpack a D-Bus session struct tuple (sssssssus) into an object.
/// Returns null if the payload is malformed or invalid.
export function unpackSessionDto(s) {
    if (!Array.isArray(s) || s.length < 9) {
        return null;
    }
    return {
        sessionId: String(s[0] || ''),
        providerId: String(s[1] || ''),
        providerDisplayName: String(s[2] || ''),
        projectName: String(s[3] || ''),
        currentState: String(s[4] || 'IDLE'),
        startedAt: String(s[5] || ''),
        stateEnteredAt: String(s[6] || ''),
        processId: Number(s[7]) || 0,
        activeToolCategory: String(s[8] || ''),
    };
}

/// Defensively sanitize workspace project names for safe presentation in desktop
/// notifications and accessibility labels. Strips control characters, newlines,
/// markup characters, limits length to 32 characters, and falls back to 'workspace'.
export function sanitizeProjectName(rawName) {
    if (!rawName || typeof rawName !== 'string') {
        return 'workspace';
    }

    // 1. Strip newlines, CR, tabs, and non-printable control characters
    let cleaned = rawName.replace(/[\x00-\x1F\x7F]/g, '');

    // 2. Strip HTML/XML/Pango markup characters (<, >, &)
    cleaned = cleaned.replace(/[<>&]/g, '');

    // 3. Trim whitespace
    cleaned = cleaned.trim();

    // 4. Truncate to maximum 32 Unicode code points (prevents splitting surrogate pairs)
    const codePoints = Array.from(cleaned);
    if (codePoints.length > 32) {
        cleaned = codePoints.slice(0, 32).join('').trim();
    }

    // 5. Fallback to 'workspace' if empty after sanitization
    if (cleaned.length === 0) {
        return 'workspace';
    }

    return cleaned;
}

/**
 * Extracts the base project or directory name from terminal window titles,
 * tab labels, or prompt strings (e.g. "hossein@ubuntu:~/Projects/WatchAI" -> "watchai",
 * "◐ watchai" -> "watchai", "Projects" -> "projects").
 */
export function extractProjectFromTitle(title) {
    if (!title || typeof title !== 'string') return '';
    let s = title.trim();
    // 1. Strip leading status emojis, runes, or symbols
    s = s.replace(/^[◐◑◒◓✳✓⚙▶■•●\s]+/, '').trim();
    // 2. If contains user@host:path, isolate path segment
    if (s.includes(':')) {
        const colonIdx = s.indexOf(':');
        s = s.substring(colonIdx + 1).trim();
    }
    // 3. Strip trailing slashes
    s = s.replace(/[/\\\\]+$/, '');
    // 4. Extract last path component
    if (s.includes('/') || s.includes('\\')) {
        const parts = s.split(/[/\\\\]/);
        s = parts[parts.length - 1] || s;
    }
    return s.toLowerCase().trim();
}

/**
 * Searches running terminal applications via AT-SPI accessibility tree to locate
 * the specific notebook tab and window corresponding to a project name and ancestor PIDs.
 * Returns an object with window reference and tab index if found, or null.
 */
export function findTerminalTabMatch(cleanProj, pids = new Set(), desktopOverride = null) {
    if (!cleanProj || typeof cleanProj !== 'string') return null;
    let target = extractProjectFromTitle(cleanProj);
    if (!target) return null;

    try {
        let desktop = desktopOverride;
        if (!desktop) {
            if (typeof Atspi === 'undefined' || typeof Atspi.get_desktop !== 'function') {
                return null;
            }
            desktop = Atspi.get_desktop(0);
        }
        if (!desktop) return null;

        function findTabList(obj, depth = 0) {
            if (!obj || depth > 8) return null;
            try {
                if (obj.get_role_name() === 'page tab list') return obj;
                const count = obj.get_child_count();
                for (let i = 0; i < count; i++) {
                    const child = obj.get_child_at_index(i);
                    const found = findTabList(child, depth + 1);
                    if (found) return found;
                }
            } catch (_) {}
            return null;
        }

        const appCount = desktop.get_child_count();
        let bestMatch = null;
        let bestScore = 0;

        for (let i = 0; i < appCount; i++) {
            const app = desktop.get_child_at_index(i);
            if (!app) continue;
            const appName = (app.get_name() || '').toLowerCase();
            let appPid = 0;
            try {
                if (typeof app.get_process_id === 'function') {
                    appPid = app.get_process_id();
                }
            } catch (_) {}

            const isAncestorPid = (appPid > 0 && pids.has(appPid));
            const isTermApp = appName.includes('terminal') || appName.includes('ptyxis') || appName.includes('console');
            if (!isAncestorPid && !isTermApp) continue;

            const winCount = app.get_child_count();
            for (let w = 0; w < winCount; w++) {
                const win = app.get_child_at_index(w);
                if (!win) continue;
                const winTitle = win.get_name() || '';
                const extractedWinProj = extractProjectFromTitle(winTitle);
                const tl = findTabList(win);

                // Handle single-tab windows or windows where tab labels are collapsed into title
                if (!tl || tl.get_child_count() <= 1) {
                    let score = 0;
                    if (extractedWinProj === target) {
                        score += 200;
                        if (isAncestorPid) score += 50;
                        if (/[◐◑◒◓✳✓⚙▶■•●]/.test(winTitle)) score += 50;
                    } else if (extractedWinProj.length > 0 && (extractedWinProj.includes(target) || target.includes(extractedWinProj))) {
                        score += 80;
                    }

                    if (score > bestScore) {
                        bestScore = score;
                        bestMatch = {
                            appPid,
                            winName: winTitle.toLowerCase(),
                            tabList: tl,
                            tabIndex: 0,
                            tabName: winTitle,
                        };
                    }
                    continue;
                }

                const tabCount = tl.get_child_count();
                for (let t = 0; t < tabCount; t++) {
                    const tab = tl.get_child_at_index(t);
                    if (!tab) continue;
                    const tabName = tab.get_name() || winTitle || '';
                    const extractedTabProj = extractProjectFromTitle(tabName);
                    let score = 0;

                    if (extractedTabProj === target) {
                        score += 200;
                        if (isAncestorPid) score += 50;
                        if (/[◐◑◒◓✳✓⚙▶■•●]/.test(tabName)) {
                            score += 50;
                        }
                    } else if (extractedTabProj.length > 0 && (extractedTabProj.includes(target) || target.includes(extractedTabProj))) {
                        score += 80;
                    }

                    if (score > bestScore) {
                        bestScore = score;
                        bestMatch = {
                            appPid,
                            winName: winTitle.toLowerCase(),
                            tabList: tl,
                            tabIndex: t,
                            tabName: tabName,
                        };
                    }
                }
            }
        }
        return bestMatch;
    } catch (e) {
        console.warn('WatchAI: Failed AT-SPI tab list inspection:', e);
        return null;
    }
}

/**
 * Focuses the desktop window associated with an agent session or process ID.
 * Traverses process ancestors to identify terminal emulators or host containers,
 * prioritizing exact project title matches across terminal windows and activating
 * the specific notebook tab in multi-tab terminals.
 */
export function activateWindowForProcess(sessionOrPid, projectNameOverride = '') {
    let pid = 0;
    let projectName = projectNameOverride;

    if (sessionOrPid && typeof sessionOrPid === 'object') {
        pid = sessionOrPid.processId || 0;
        projectName = sessionOrPid.projectName || projectNameOverride || '';
    } else if (typeof sessionOrPid === 'number') {
        pid = sessionOrPid;
    }

    if (!pid || pid <= 0) return false;

    // Collect process ancestors, terminating before session manager / systemd
    const pids = new Set();
    let currentPid = pid;
    for (let depth = 0; depth < 10 && currentPid > 1; depth++) {
        pids.add(currentPid);
        try {
            const [, statBytes] = GLib.file_get_contents(`/proc/${currentPid}/stat`);
            if (!statBytes) break;
            const str = new TextDecoder().decode(statBytes);
            const closeParen = str.lastIndexOf(')');
            if (closeParen === -1) break;
            const comm = str.substring(str.indexOf('(') + 1, closeParen);
            if (comm === 'systemd' || comm === 'init' || comm === 'gnome-session') {
                break;
            }
            const fields = str.substring(closeParen + 2).trim().split(/\s+/);
            const ppid = parseInt(fields[1], 10);
            if (ppid > 1) {
                currentPid = ppid;
            } else {
                break;
            }
        } catch {
            break;
        }
    }

    if (typeof globalThis.global === 'undefined' || typeof global.get_window_actors !== 'function') {
        return false;
    }

    try {
        const windowActors = global.get_window_actors();
        const termKeywords = ['term', 'ptyxis', 'kitty', 'alacritty', 'konsole', 'xterm', 'code', 'vscodium', 'cursor'];
        const cleanProj = extractProjectFromTitle(projectName);

        // 1. Probe AT-SPI for terminal tabs matching project name and ancestor PIDs
        const tabMatch = findTerminalTabMatch(cleanProj, pids);

        let bestWindow = null;
        let bestScore = 0;

        for (const actor of windowActors) {
            const metaWindow = actor.get_meta_window();
            if (!metaWindow) continue;

            const wPid = metaWindow.get_pid();
            const wTitle = (metaWindow.get_title() || '').toLowerCase();
            const wmClass = (metaWindow.get_wm_class() || '').toLowerCase();
            const extractedWinProj = extractProjectFromTitle(wTitle);

            const isDirect = (wPid === pid);
            const isAncestor = pids.has(wPid);
            const isTerm = termKeywords.some(k => wmClass.includes(k));
            const hasProject = cleanProj.length > 0 && (extractedWinProj === cleanProj || extractedWinProj.includes(cleanProj));

            let score = 0;

            // Highest priority: AT-SPI identified this window as housing the matching project tab!
            if (tabMatch && (wPid === tabMatch.appPid || isAncestor || isTerm)) {
                if (tabMatch.winName && (wTitle === tabMatch.winName || extractProjectFromTitle(wTitle) === extractProjectFromTitle(tabMatch.winName))) {
                    score += 20000;
                }
            }

            if (isDirect) {
                score += 10000;
            } else if (isAncestor) {
                score += 5000;
            }

            if (isAncestor && isTerm) {
                score += 1000;
            }

            if (hasProject) {
                if (isAncestor) {
                    score += 4000; // Exact project title match in ancestor terminal!
                } else if (isTerm) {
                    score += 2500;
                }
            }

            if (score > bestScore) {
                bestScore = score;
                bestWindow = metaWindow;
            }
        }

        if (bestWindow) {
            const workspace = bestWindow.get_workspace();
            if (workspace) {
                workspace.activate(global.get_current_time());
            }
            if (bestWindow.minimized) {
                bestWindow.unminimize();
            }
            bestWindow.activate(global.get_current_time());

            if (typeof globalThis.Main !== 'undefined' && typeof globalThis.Main.activateWindow === 'function') {
                globalThis.Main.activateWindow(bestWindow);
            }

            // 2. Activate the matching tab in multi-tab terminals
            if (tabMatch && tabMatch.tabList && typeof tabMatch.tabList.select_child === 'function' && tabMatch.tabIndex >= 0) {
                try {
                    tabMatch.tabList.select_child(tabMatch.tabIndex);
                } catch (tabErr) {
                    console.warn('WatchAI: Failed to activate terminal tab index ' + tabMatch.tabIndex, tabErr);
                }
            }

            return true;
        }
    } catch (e) {
        console.warn('WatchAI: Failed to activate window for PID ' + pid, e);
    }

    return false;
}
