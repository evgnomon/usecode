import Shell from 'gi://Shell';
import Gio from 'gi://Gio';

const DBUS_IFACE = `
<node>
  <interface name="org.evgnomon.FzURLs">
    <method name="FocusBraveOnActiveWorkspace">
      <arg type="b" direction="out" name="found"/>
    </method>
  </interface>
</node>`;

export default class FzURLsFocusExtension {
    _focusBraveOnActiveWorkspace() {
        const activeIndex = global.workspace_manager.get_active_workspace_index();
        const tracker = Shell.WindowTracker.get_default();

        for (const actor of global.get_window_actors()) {
            const meta = actor.get_meta_window();
            if (!meta || meta.is_skip_taskbar())
                continue;

            const app = tracker.get_window_app(meta);
            if (!app)
                continue;

            if (app.get_id() !== 'brave-browser.desktop')
                continue;

            if (meta.get_workspace().index() === activeIndex) {
                meta.activate(global.get_current_time());
                return true;
            }
        }
        return false;
    }

    enable() {
        const self = this;
        this._dbusObj = {
            FocusBraveOnActiveWorkspace() {
                return self._focusBraveOnActiveWorkspace();
            },
        };
        this._dbusImpl = Gio.DBusExportedObject.wrapJSObject(DBUS_IFACE, this._dbusObj);
        this._dbusImpl.export(Gio.DBus.session, '/org/evgnomon/FzURLs');

        this._ownerId = Gio.DBus.session.own_name(
            'org.evgnomon.FzURLs',
            Gio.BusNameOwnerFlags.NONE,
            null,
            null,
        );
    }

    disable() {
        if (this._dbusImpl) {
            this._dbusImpl.unexport();
            this._dbusImpl = null;
        }
        if (this._ownerId) {
            Gio.DBus.session.unown_name(this._ownerId);
            this._ownerId = null;
        }
    }
}
