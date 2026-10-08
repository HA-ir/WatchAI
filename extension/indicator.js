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

        // Primary badge group
        this._primaryBox = new St.BoxLayout({
            style_class: 'watchai-status-group',
            y_align: Clutter.ActorAlign.CENTER,
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

        this._primaryBox.add_child(this._icon);
        this._primaryBox.add_child(this._countLabel);

        // Auxiliary badge groups for coexisting multi-state display (Approach A)
        // 1. Auxiliary Waiting Badge (Pause / Warning)
        this._auxWaitingBox = new St.BoxLayout({
            style_class: 'watchai-status-group watchai-aux-badge',
            y_align: Clutter.ActorAlign.CENTER,
            visible: false,
        });
        this._auxWaitingIcon = new St.Icon({
            icon_name: 'dialog-warning-symbolic',
            style_class: 'system-status-icon watchai-status-icon watchai-icon-symbolic watchai-state-waiting',
        });
        this._auxWaitingLabel = new St.Label({
            text: '',
            style_class: 'watchai-aux-label',
            y_align: Clutter.ActorAlign.CENTER,
        });
        this._auxWaitingBox.add_child(this._auxWaitingIcon);
        this._auxWaitingBox.add_child(this._auxWaitingLabel);

        // 2. Auxiliary Working Badge (Play/Executing)
        this._auxWorkingBox = new St.BoxLayout({
            style_class: 'watchai-status-group watchai-aux-badge',
            y_align: Clutter.ActorAlign.CENTER,
            visible: false,
        });
        this._auxWorkingIcon = new St.Icon({
            icon_name: 'media-playback-start-symbolic',
            style_class: 'system-status-icon watchai-status-icon watchai-icon-symbolic watchai-state-working',
        });
        this._auxWorkingLabel = new St.Label({
            text: '',
            style_class: 'watchai-aux-label',
            y_align: Clutter.ActorAlign.CENTER,
        });
        this._auxWorkingBox.add_child(this._auxWorkingIcon);
        this._auxWorkingBox.add_child(this._auxWorkingLabel);

        // 3. Auxiliary Success Badge (Checkmark)
        this._auxSuccessBox = new St.BoxLayout({
            style_class: 'watchai-status-group watchai-aux-badge',
            y_align: Clutter.ActorAlign.CENTER,
            visible: false,
        });
        this._auxSuccessIcon = new St.Icon({
            icon_name: 'emblem-ok-symbolic',
            style_class: 'system-status-icon watchai-status-icon watchai-icon-symbolic watchai-state-success',
        });
        this._auxSuccessLabel = new St.Label({
            text: '',
            style_class: 'watchai-aux-label',
            y_align: Clutter.ActorAlign.CENTER,
        });
        this._auxSuccessBox.add_child(this._auxSuccessIcon);
        this._auxSuccessBox.add_child(this._auxSuccessLabel);

        // 4. Auxiliary Error Badge (Error Exclamation)
        this._auxErrorBox = new St.BoxLayout({
            style_class: 'watchai-status-group watchai-aux-badge',
            y_align: Clutter.ActorAlign.CENTER,
            visible: false,
        });
        this._auxErrorIcon = new St.Icon({
            icon_name: 'dialog-error-symbolic',
            style_class: 'system-status-icon watchai-status-icon watchai-icon-symbolic watchai-state-error',
        });
        this._auxErrorLabel = new St.Label({
            text: '',
            style_class: 'watchai-aux-label',
            y_align: Clutter.ActorAlign.CENTER,
        });
        this._auxErrorBox.add_child(this._auxErrorIcon);
        this._auxErrorBox.add_child(this._auxErrorLabel);

        this._box.add_child(this._primaryBox);
        this._box.add_child(this._auxWaitingBox);
        this._box.add_child(this._auxWorkingBox);
        this._box.add_child(this._auxSuccessBox);
        this._box.add_child(this._auxErrorBox);
        this.add_child(this._box);

        // Attach session inspection popover to button menu
        this._popover = new WatchAISessionPopover(this.menu, this._settings);

        this._currentState = 'IDLE';
        this._activeCount = 0;
        this._waitingCount = 0;
        this._errorCount = 0;
        this._workingCount = 0;
        this._successCount = 0;

        // Listen for dynamic icon style preference updates (T063, FR-008)
        if (this._settings && typeof this._settings.onChanged === 'function') {
            this._iconStyleChangedId = this._settings.onChanged('indicator-icon-style', () => {
                this._reapplyIconStyle();
            });
        }

        this._reapplyIconStyle();
    }

    get popover() {
        return this._popover;
    }

    get notifications() {
        return this._notifications;
    }

    _setA11yRole(actor, role) {
        if (!actor) return;
        if (typeof actor.set_accessible_role === 'function') {
            actor.set_accessible_role(role);
        } else if (typeof actor.get_accessible === 'function') {
            const acc = actor.get_accessible();
            if (acc && typeof acc.set_role === 'function') {
                acc.set_role(role);
            }
        }
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
            this._errorCount,
            this._workingCount,
            this._successCount
        );
    }

    updateState(state, activeCount = 0, waitingCount = 0, errorCount = 0, workingCount = 0, successCount = 0) {
        this._currentState = state;
        this._activeCount = activeCount;
        this._waitingCount = waitingCount;
        this._errorCount = errorCount;
        this._workingCount = workingCount;
        this._successCount = successCount;

        let effectiveWorking = workingCount;
        let effectiveWaiting = waitingCount;
        let effectiveSuccess = successCount;
        let effectiveError = errorCount;

        // Defensive fallback for legacy or direct calls without granular counts
        if (activeCount > 0 && workingCount === 0 && waitingCount === 0 && errorCount === 0 && successCount === 0) {
            if (state === 'WORKING') effectiveWorking = activeCount;
            else if (state === 'WAITING') effectiveWaiting = activeCount;
            else if (state === 'SUCCESS') effectiveSuccess = activeCount;
            else if (state === 'ERROR') effectiveError = activeCount;
        }

        const hasWorking = effectiveWorking > 0;
        const hasWaiting = effectiveWaiting > 0;
        const hasSuccess = effectiveSuccess > 0;
        const hasError = effectiveError > 0;

        const categoryCount = (hasWorking ? 1 : 0) + (hasWaiting ? 1 : 0) + (hasSuccess ? 1 : 0) + (hasError ? 1 : 0);
        const modeClass = this._getIconModeClass();

        if (categoryCount > 1) {
            // Coexisting states (Approach A): Segmented status badges in top bar
            let primaryState = 'WORKING';
            let primaryCount = effectiveWorking;

            if (hasError) {
                primaryState = 'ERROR';
                primaryCount = effectiveError;
            } else if (hasWaiting) {
                primaryState = 'WAITING';
                primaryCount = effectiveWaiting;
            } else if (hasWorking) {
                primaryState = 'WORKING';
                primaryCount = effectiveWorking;
            } else if (hasSuccess) {
                primaryState = 'SUCCESS';
                primaryCount = effectiveSuccess;
            }

            const primaryConfig = STATE_CONFIG[primaryState] || STATE_CONFIG.UNKNOWN;
            this._icon.icon_name = primaryConfig.icon;
            this._icon.style_class = `system-status-icon watchai-status-icon ${modeClass} ${primaryConfig.cssClass}`;
            this._countLabel.text = `${primaryCount}`;
            this._countLabel.visible = true;
            this._setA11yRole(this._countLabel, Atk.Role.LABEL);

            // 1. Auxiliary Waiting Badge
            if (hasWaiting && primaryState !== 'WAITING') {
                this._auxWaitingBox.visible = true;
                this._auxWaitingLabel.text = `${effectiveWaiting}`;
                this._auxWaitingIcon.style_class = `system-status-icon watchai-status-icon ${modeClass} watchai-state-waiting`;
                this._setA11yRole(this._auxWaitingLabel, Atk.Role.LABEL);
            } else {
                this._auxWaitingBox.visible = false;
                this._setA11yRole(this._auxWaitingLabel, Atk.Role.INVALID);
            }

            // 2. Auxiliary Working Badge
            if (hasWorking && primaryState !== 'WORKING') {
                this._auxWorkingBox.visible = true;
                this._auxWorkingLabel.text = `${effectiveWorking}`;
                this._auxWorkingIcon.style_class = `system-status-icon watchai-status-icon ${modeClass} watchai-state-working`;
                this._setA11yRole(this._auxWorkingLabel, Atk.Role.LABEL);
            } else {
                this._auxWorkingBox.visible = false;
                this._setA11yRole(this._auxWorkingLabel, Atk.Role.INVALID);
            }

            // 3. Auxiliary Success Badge
            if (hasSuccess && primaryState !== 'SUCCESS') {
                this._auxSuccessBox.visible = true;
                this._auxSuccessLabel.text = `${effectiveSuccess}`;
                this._auxSuccessIcon.style_class = `system-status-icon watchai-status-icon ${modeClass} watchai-state-success`;
                this._setA11yRole(this._auxSuccessLabel, Atk.Role.LABEL);
            } else {
                this._auxSuccessBox.visible = false;
                this._setA11yRole(this._auxSuccessLabel, Atk.Role.INVALID);
            }

            // 4. Auxiliary Error Badge
            if (hasError && primaryState !== 'ERROR') {
                this._auxErrorBox.visible = true;
                this._auxErrorLabel.text = `${effectiveError}`;
                this._auxErrorIcon.style_class = `system-status-icon watchai-status-icon ${modeClass} watchai-state-error`;
                this._setA11yRole(this._auxErrorLabel, Atk.Role.LABEL);
            } else {
                this._auxErrorBox.visible = false;
                this._setA11yRole(this._auxErrorLabel, Atk.Role.INVALID);
            }

            // Descriptive AT-SPI multi-status accessible name
            const details = [];
            if (hasWorking) details.push(`${effectiveWorking} working`);
            if (hasWaiting) details.push(`${effectiveWaiting} waiting for input`);
            if (hasError) details.push(`${effectiveError} in error`);
            if (hasSuccess) details.push(`${effectiveSuccess} completed`);
            this.set_accessible_name(`WatchAI: ${details.join(', ')} (${activeCount} active sessions)`);
        } else {
            // Single category or Idle / Offline
            this._auxWaitingBox.visible = false;
            this._auxWorkingBox.visible = false;
            this._auxSuccessBox.visible = false;
            this._auxErrorBox.visible = false;
            this._setA11yRole(this._auxWaitingLabel, Atk.Role.INVALID);
            this._setA11yRole(this._auxWorkingLabel, Atk.Role.INVALID);
            this._setA11yRole(this._auxSuccessLabel, Atk.Role.INVALID);
            this._setA11yRole(this._auxErrorLabel, Atk.Role.INVALID);

            const config = STATE_CONFIG[state] || STATE_CONFIG.UNKNOWN;
            this._icon.icon_name = config.icon;
            this._icon.style_class = `system-status-icon watchai-status-icon ${modeClass} ${config.cssClass}`;

            if (activeCount > 1) {
                this._countLabel.text = `${activeCount}`;
                this._countLabel.visible = true;
                this._setA11yRole(this._countLabel, Atk.Role.LABEL);
            } else {
                this._countLabel.visible = false;
                this._setA11yRole(this._countLabel, Atk.Role.INVALID);
            }

            let a11yText = config.accessibleDesc;
            if (activeCount > 1) {
                a11yText += ` (${activeCount} active sessions)`;
            }
            this.set_accessible_name(a11yText);
        }
    }

    setDisconnected() {
        this._currentState = 'OFFLINE';
        const modeClass = this._getIconModeClass();
        this._icon.icon_name = 'system-run-symbolic';
        this._icon.style_class = `system-status-icon watchai-status-icon ${modeClass} watchai-state-offline`;
        this._countLabel.visible = false;
        this._auxWaitingBox.visible = false;
        this._auxWorkingBox.visible = false;
        this._auxSuccessBox.visible = false;
        this._auxErrorBox.visible = false;
        this._setA11yRole(this._countLabel, Atk.Role.INVALID);
        this._setA11yRole(this._auxWaitingLabel, Atk.Role.INVALID);
        this._setA11yRole(this._auxWorkingLabel, Atk.Role.INVALID);
        this._setA11yRole(this._auxSuccessLabel, Atk.Role.INVALID);
        this._setA11yRole(this._auxErrorLabel, Atk.Role.INVALID);
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
