import St from 'gi://St';
import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import { formatDuration, getStatePriority } from './utils.js';

export { formatDuration, getStatePriority };

export class WatchAISessionCard {
    constructor(session) {
        this.session = session;
        this.actor = new St.BoxLayout({
            vertical: true,
            style_class: `watchai-session-card watchai-card-${session.currentState.toLowerCase()}`,
            reactive: true,
        });

        // Top Row: Provider, State Badge, Elapsed Duration
        const topRow = new St.BoxLayout({
            style_class: 'watchai-card-top-row',
        });

        this._providerLabel = new St.Label({
            text: session.providerDisplayName || session.providerId,
            style_class: 'watchai-session-title',
            x_expand: true,
        });

        this._stateBadge = new St.Label({
            text: session.currentState,
            style_class: `watchai-state-badge watchai-badge-${session.currentState.toLowerCase()}`,
            y_align: Clutter.ActorAlign.CENTER,
        });

        this._durationLabel = new St.Label({
            text: formatDuration(session.startedAt),
            style_class: 'watchai-duration-label',
            y_align: Clutter.ActorAlign.CENTER,
        });

        topRow.add_child(this._providerLabel);
        topRow.add_child(this._stateBadge);
        topRow.add_child(this._durationLabel);

        // Bottom Row: Workspace Name, PID, Active Tool
        const bottomRow = new St.BoxLayout({
            style_class: 'watchai-card-bottom-row',
        });

        let detailText = `📂 ${session.projectName || 'workspace'}`;
        if (session.processId && session.processId > 0) {
            detailText += `  •  PID: ${session.processId}`;
        }
        if (session.activeToolCategory && session.activeToolCategory.length > 0) {
            detailText += `  •  Tool: ${session.activeToolCategory}`;
        }

        this._detailLabel = new St.Label({
            text: detailText,
            style_class: 'watchai-session-subtitle',
            x_expand: true,
        });

        bottomRow.add_child(this._detailLabel);

        this.actor.add_child(topRow);
        this.actor.add_child(bottomRow);
    }

    update(session) {
        this.session = session;
        this._providerLabel.text = session.providerDisplayName || session.providerId;
        this._stateBadge.text = session.currentState;
        this._stateBadge.style_class = `watchai-state-badge watchai-badge-${session.currentState.toLowerCase()}`;
        this.actor.style_class = `watchai-session-card watchai-card-${session.currentState.toLowerCase()}`;

        let detailText = `📂 ${session.projectName || 'workspace'}`;
        if (session.processId && session.processId > 0) {
            detailText += `  •  PID: ${session.processId}`;
        }
        if (session.activeToolCategory && session.activeToolCategory.length > 0) {
            detailText += `  •  Tool: ${session.activeToolCategory}`;
        }
        this._detailLabel.text = detailText;
        this.updateDuration();
    }

    updateDuration() {
        this._durationLabel.text = formatDuration(this.session.startedAt);
    }
}

export class WatchAISessionPopover {
    constructor(menu) {
        this._menu = menu;
        this._cards = new Map(); // sessionId -> WatchAISessionCard
        this._durationTimerId = null;

        this._buildUI();
    }

    _buildUI() {
        // Section Header
        this._headerSection = new PopupMenu.PopupMenuSection();
        const headerBox = new St.BoxLayout({ style_class: 'watchai-popover-header' });
        this._titleLabel = new St.Label({
            text: 'WatchAI Agent Sessions',
            style_class: 'watchai-header-title',
            x_expand: true,
        });
        headerBox.add_child(this._titleLabel);
        this._headerSection.actor.add_child(headerBox);
        this._menu.addMenuItem(this._headerSection);

        this._menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());

        // Empty State Placeholder
        this._emptySection = new PopupMenu.PopupMenuSection();
        const emptyBox = new St.BoxLayout({
            style_class: 'watchai-empty-state-box',
            vertical: true,
        });
        this._emptyLabel = new St.Label({
            text: 'No active coding agents observed.',
            style_class: 'watchai-empty-label',
        });
        emptyBox.add_child(this._emptyLabel);
        this._emptySection.actor.add_child(emptyBox);
        this._menu.addMenuItem(this._emptySection);

        // Session Cards Section
        this._cardsSection = new PopupMenu.PopupMenuSection();
        this._menu.addMenuItem(this._cardsSection);

        // Start/Stop duration timer on menu open/close
        this._openStateChangedId = this._menu.connect('open-state-changed', (_menu, isOpen) => {
            if (isOpen) {
                this._startDurationTimer();
            } else {
                this._stopDurationTimer();
            }
        });

        this._updateEmptyState();
    }

    _startDurationTimer() {
        this._stopDurationTimer();
        this._tickDurations();
        this._durationTimerId = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 1, () => {
            this._tickDurations();
            return GLib.SOURCE_CONTINUE;
        });
    }

    _stopDurationTimer() {
        if (this._durationTimerId) {
            GLib.source_remove(this._durationTimerId);
            this._durationTimerId = null;
        }
    }

    _tickDurations() {
        for (const card of this._cards.values()) {
            card.updateDuration();
        }
    }

    _updateEmptyState() {
        const isEmpty = this._cards.size === 0;
        this._emptySection.actor.visible = isEmpty;
        this._cardsSection.actor.visible = !isEmpty;
    }

    _reorderCards() {
        // Sort cards: Priority descending (WAITING/ERROR first), then startedAt descending
        const sorted = Array.from(this._cards.values()).sort((a, b) => {
            const prioA = getStatePriority(a.session.currentState);
            const prioB = getStatePriority(b.session.currentState);
            if (prioA !== prioB) {
                return prioB - prioA;
            }
            return (b.session.startedAt || '').localeCompare(a.session.startedAt || '');
        });

        // Re-append in sorted order
        for (const card of sorted) {
            this._cardsSection.actor.set_child_above_sibling(card.actor, null);
        }
    }

    setSessions(sessions) {
        // Clear existing cards
        for (const card of this._cards.values()) {
            this._cardsSection.actor.remove_child(card.actor);
        }
        this._cards.clear();

        for (const s of sessions) {
            this.addSession(s);
        }
    }

    addSession(session) {
        if (this._cards.has(session.sessionId)) {
            this.updateSession(session);
            return;
        }

        const card = new WatchAISessionCard(session);
        this._cards.set(session.sessionId, card);
        this._cardsSection.actor.add_child(card.actor);

        this._reorderCards();
        this._updateEmptyState();
    }

    updateSession(session) {
        const card = this._cards.get(session.sessionId);
        if (card) {
            card.update(session);
            this._reorderCards();
            this._updateEmptyState();
        } else {
            this.addSession(session);
        }
    }

    removeSession(sessionId) {
        const card = this._cards.get(sessionId);
        if (card) {
            this._cardsSection.actor.remove_child(card.actor);
            this._cards.delete(sessionId);
            this._updateEmptyState();
        }
    }

    destroy() {
        this._stopDurationTimer();
        if (this._openStateChangedId) {
            this._menu.disconnect(this._openStateChangedId);
            this._openStateChangedId = null;
        }
        this._cards.clear();
    }
}
