import Adw from 'gi://Adw';
import Gtk from 'gi://Gtk';
import { ExtensionPreferences, gettext as _ } from 'resource:///org/gnome/Shell/Extensions/js/extensions/prefs.js';

export default class WatchAIPreferences extends ExtensionPreferences {
    fillPreferencesWindow(window) {
        const settings = this.getSettings();

        const page = new Adw.PreferencesPage();
        window.add(page);

        // Group 1: Desktop Notifications
        const notifGroup = new Adw.PreferencesGroup({
            title: _('Desktop Notifications'),
            description: _('Configure alerts for AI agent state transitions'),
        });
        page.add(notifGroup);

        const enableNotifRow = new Adw.SwitchRow({
            title: _('Enable Desktop Notifications'),
            subtitle: _('Master switch for all WatchAI desktop notifications'),
        });
        notifGroup.add(enableNotifRow);
        settings.bind('enable-desktop-notifications', enableNotifRow, 'active', 0);

        const waitingRow = new Adw.SwitchRow({
            title: _('Notify on Waiting for Approval'),
            subtitle: _('Alert when an agent is blocked waiting for user input or tool confirmation'),
        });
        notifGroup.add(waitingRow);
        settings.bind('notify-on-waiting', waitingRow, 'active', 0);
        settings.bind('enable-desktop-notifications', waitingRow, 'sensitive', 0);

        const errorRow = new Adw.SwitchRow({
            title: _('Notify on Error or Crash'),
            subtitle: _('Alert when an agent command fails or process crashes'),
        });
        notifGroup.add(errorRow);
        settings.bind('notify-on-error', errorRow, 'active', 0);
        settings.bind('enable-desktop-notifications', errorRow, 'sensitive', 0);

        // Group 2: Top-Bar Indicator Appearance
        const indicatorGroup = new Adw.PreferencesGroup({
            title: _('Top-Bar Indicator'),
            description: _('Customize indicator presentation and dwell timing'),
        });
        page.add(indicatorGroup);

        const iconStyleRow = new Adw.ComboRow({
            title: _('Icon Presentation Style'),
            subtitle: _('Monochrome desktop theme vs vibrant state-colored accents'),
            model: new Gtk.StringList({
                strings: [_('Symbolic (Monochrome)'), _('Colored (Accents)')],
            }),
        });
        const currentStyle = settings.get_string('indicator-icon-style');
        iconStyleRow.selected = currentStyle === 'colored' ? 1 : 0;
        iconStyleRow.connect('notify::selected', () => {
            settings.set_string(
                'indicator-icon-style',
                iconStyleRow.selected === 1 ? 'colored' : 'symbolic'
            );
        });
        indicatorGroup.add(iconStyleRow);

        const dwellRow = new Adw.SpinRow({
            title: _('Completion Dwell Duration (seconds)'),
            subtitle: _('Time finished agent tasks dwell in Success before resetting to Idle (10–60s)'),
            adjustment: new Gtk.Adjustment({
                lower: 10,
                upper: 60,
                step_increment: 5,
                page_increment: 10,
                value: settings.get_uint('dwell-duration-seconds'),
            }),
        });
        settings.bind('dwell-duration-seconds', dwellRow, 'value', 0);
        indicatorGroup.add(dwellRow);
    }
}
