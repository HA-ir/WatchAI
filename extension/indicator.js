import Atk from 'gi://Atk';
import GObject from 'gi://GObject';
import St from 'gi://St';
import Clutter from 'gi://Clutter';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import { WatchAISessionPopover } from './popover.js';
import { NotificationManager } from './notifications.js';
import { STATE_CONFIG } from './utils.js';

export { STATE_CONFIG };

export const WatchAIIndicator = GObject.registerClass(
class WatchAIIndicator extends PanelMenu.Button {
    _init(settings = null, notifications = null) {
        super._init(0.0, 'WatchAI Indicator', false);

        this._settings = settings;
        this._notifications = notifications || new NotificationManager(this._settings);
        this._iconStyleChangedId = null;

        // Set AT-SPI role: TOGGLE_BUTTON (T065, FR-020)
        // Prefer direct actor API, falling back defensively to get_accessible().set_role()
        if (typeof this.set_accessible_role === 'function') {
            this.set_accessible_role(Atk.Role.TOGGLE_BUTTON);
        } else if (typeof this.get_accessible === 'function') {
            const acc = this.get_accessible();
            if (acc && typeof acc.set_role === 'function') {
                acc.set_role(Atk.Role.TOGGLE_BUTTON);
            }
        }

        // Set AT-SPI accessible description (T065, FR-023)
        if (typeof this.get_accessible === 'function') {
            try {
                const acc = this.get_accessible();
                if (acc && !(acc instanceof Atk.Action) && typeof acc.set_description === 'function') {
                    acc.set_description('Click to open agent session popover menu');
                }
            } catch (_) {
                // Defensive: Ignore GJS ATK interface dispatch conflicts
            }
        }

        this.set_accessible_name('WatchAI Agent Indicator');

        this._box = new St.BoxLayout({
            style_class: 'watchai-indicator-box',
            reactive: true,
        });

        this._icon = new St.Icon({
            icon_name: 'system-run-symbolic',
            style_class: 'system-status-icon watchai-status-icon watchai-icon-symbolic watchai-state-idle',
        });

        this._countLabel = new St.Label({
            text: '',
            y_align: Clutter.ActorAlign.CENTER,
            visible: false,
        });

        this._box.add_child(this._icon);
        this._box.add_child(this._countLabel);
        this.add_child(this._box);

        // Attach session inspection popover to button menu
        this._popover = new WatchAISessionPopover(this.menu);

        this._currentState = 'IDLE';
        this._activeCount = 0;
        this._waitingCount = 0;
        this._errorCount = 0;

        // Listen for dynamic icon style preference updates (T063, FR-008)
        if (this._settings && typeof this._settings.onChanged === 'function') {
            this._iconStyleChangedId = this._settings.onChanged('indicator-icon-style', () => {
                this._reapplyIconStyle();
            });
        }

        this.updateState('IDLE', 0, 0, 0);
    }

    get popover() {
        return this._popover;
    }

    get notifications() {
        return this._notifications;
    }

    _getIconModeClass() {
        const style = this._settings ? this._settings.getIconStyle() : 'symbolic';
        return style === 'colored' ? 'watchai-icon-colored' : 'watchai-icon-symbolic';
    }

    _reapplyIconStyle() {
        if (this._currentState === 'OFFLINE') {
            const modeClass = this._getIconModeClass();
            this._icon.style_class = `system-status-icon watchai-status-icon ${modeClass} watchai-state-offline`;
            return;
        }
        this.updateState(
            this._currentState,
            this._activeCount,
            this._waitingCount,
            this._errorCount
        );
    }

    updateState(state, activeCount = 0, waitingCount = 0, errorCount = 0) {
        this._currentState = state;
        this._activeCount = activeCount;
        this._waitingCount = waitingCount;
        this._errorCount = errorCount;

        const config = STATE_CONFIG[state] || STATE_CONFIG.UNKNOWN;
        const modeClass = this._getIconModeClass();

        // Reset classes and apply matching style with icon presentation mode
        this._icon.icon_name = config.icon;
        this._icon.style_class = `system-status-icon watchai-status-icon ${modeClass} ${config.cssClass}`;

        // Set AT-SPI accessible name (T065, FR-020, FR-021)
        let a11yText = config.accessibleDesc;
        if (activeCount > 1) {
            a11yText += ` (${activeCount} active sessions)`;
        }
        this.set_accessible_name(a11yText);

        // Show session count badge if multiple sessions active, and manage AT-SPI visibility (FR-027)
        if (activeCount > 1) {
            this._countLabel.text = `${activeCount}`;
            this._countLabel.visible = true;
            if (typeof this._countLabel.set_accessible_role === 'function') {
                this._countLabel.set_accessible_role(Atk.Role.LABEL);
            } else if (typeof this._countLabel.get_accessible === 'function') {
                const badgeAcc = this._countLabel.get_accessible();
                if (badgeAcc && typeof badgeAcc.set_role === 'function') {
                    badgeAcc.set_role(Atk.Role.LABEL);
                }
            }
        } else {
            this._countLabel.visible = false;
            // Exclude from assistive tree when <= 1 to prevent redundant announcement
            if (typeof this._countLabel.set_accessible_role === 'function') {
                this._countLabel.set_accessible_role(Atk.Role.INVALID);
            } else if (typeof this._countLabel.get_accessible === 'function') {
                const badgeAcc = this._countLabel.get_accessible();
                if (badgeAcc && typeof badgeAcc.set_role === 'function') {
                    badgeAcc.set_role(Atk.Role.INVALID);
                }
            }
        }
    }

    setDisconnected() {
        this._currentState = 'OFFLINE';
        const modeClass = this._getIconModeClass();
        this._icon.icon_name = 'system-run-symbolic';
        this._icon.style_class = `system-status-icon watchai-status-icon ${modeClass} watchai-state-offline`;
        this._countLabel.visible = false;
        this.set_accessible_name('WatchAI daemon offline');
        if (this._popover) {
            this._popover.setOfflineMode(true);
        }
    }

    notifySessionUpdated(session) {
        if (this._notifications) {
            this._notifications.handleSessionUpdated(session);
        }
    }

    notifySessionRemoved(sessionId) {
        if (this._notifications) {
            this._notifications.cleanupSession(sessionId);
        }
    }

    destroy() {
        if (this._settings && this._iconStyleChangedId) {
            if (typeof this._settings.disconnect === 'function') {
                this._settings.disconnect(this._iconStyleChangedId);
            }
            this._iconStyleChangedId = null;
        }
        if (this._notifications) {
            this._notifications.destroy();
            this._notifications = null;
        }
        if (this._popover) {
            this._popover.destroy();
            this._popover = null;
        }
        this._settings = null;
        super.destroy();
    }
});
