import GObject from 'gi://GObject';
import St from 'gi://St';
import Clutter from 'gi://Clutter';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import { WatchAISessionPopover } from './popover.js';

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

export const WatchAIIndicator = GObject.registerClass(
class WatchAIIndicator extends PanelMenu.Button {
    _init() {
        super._init(0.0, 'WatchAI Indicator', false);

        this.set_accessible_name('WatchAI Agent Indicator');

        this._box = new St.BoxLayout({
            style_class: 'watchai-indicator-box',
            reactive: true,
        });

        this._icon = new St.Icon({
            icon_name: 'system-run-symbolic',
            style_class: 'system-status-icon watchai-status-icon watchai-state-idle',
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
        this.updateState('IDLE', 0, 0, 0);
    }

    get popover() {
        return this._popover;
    }

    updateState(state, activeCount = 0, waitingCount = 0, errorCount = 0) {
        this._currentState = state;
        const config = STATE_CONFIG[state] || STATE_CONFIG.UNKNOWN;

        // Reset classes and apply matching style
        this._icon.icon_name = config.icon;
        this._icon.style_class = `system-status-icon watchai-status-icon ${config.cssClass}`;

        // Set AT-SPI accessible description
        let a11yText = config.accessibleDesc;
        if (activeCount > 1) {
            a11yText += ` (${activeCount} active sessions)`;
        }
        this.set_accessible_name(a11yText);

        // Show session count badge if multiple sessions active
        if (activeCount > 1) {
            this._countLabel.text = `${activeCount}`;
            this._countLabel.visible = true;
        } else {
            this._countLabel.visible = false;
        }
    }

    setDisconnected() {
        this._currentState = 'OFFLINE';
        this._icon.icon_name = 'system-run-symbolic';
        this._icon.style_class = 'system-status-icon watchai-status-icon watchai-state-offline';
        this._countLabel.visible = false;
        this.set_accessible_name('WatchAI daemon offline');
        if (this._popover) {
            this._popover.setOfflineMode(true);
        }
    }

    destroy() {
        if (this._popover) {
            this._popover.destroy();
            this._popover = null;
        }
        super.destroy();
    }
});
