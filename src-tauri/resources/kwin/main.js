/*
 * Neutron Premiere Home Overlay fix (client-side-decoration build)
 *
 * Premiere's Home/Welcome screen is a separate, user-movable window (it has its
 * own title bar by Adobe's design) stacked on top of the main window. Under
 * Wayland the app cannot position its own toplevels, so the compositor places it
 * at the top, covering the main window's menu bar.
 *
 * With client-side decorations (Wine draws its own NC, no compositor frame, no
 * overflow) there is nothing to resize or de-border here — we only need to do
 * what X11 does for free: place the overlay just below the main window's menu bar
 * so both the menu bar and the overlay's own title bar are visible, matching the
 * native Windows layout.
 *
 * The overlay is identified as a Premiere-class window that is LARGE (excludes the
 * small splash/dialogs) and has an EMPTY caption (the main/editor windows carry a
 * non-empty caption).
 */
(function () {
    "use strict";

    var OFFSET = 48;          /*NEUTRON_OFFSET*/   // logical px below the work-area top
    var MIN_W = 1500;
    var MIN_H = 1000;
    var TOL = 3;

    function log(m) { print("NEUTRON_HOME " + m); }

    function isOverlay(w) {
        if (!w || !w.resourceClass) return false;
        if (String(w.resourceClass).toLowerCase().indexOf("premiere") === -1) return false;
        var g = w.frameGeometry;
        if (g.width < MIN_W || g.height < MIN_H) return false;
        return !w.caption || String(w.caption).length === 0;
    }

    function near(a, b) { return Math.abs(a - b) <= TOL; }

    function place(w) {
        if (!isOverlay(w)) return;
        var a = workspace.clientArea(KWin.MaximizeArea, w);
        var g = w.frameGeometry;
        var x = a.x, y = a.y + OFFSET;
        if (near(g.x, x) && near(g.y, y)) return;
        log("overlay @" + Math.round(g.x) + "," + Math.round(g.y) + " -> @" + x + "," + y);
        w.frameGeometry = { x: x, y: y, width: g.width, height: g.height };
    }

    function hook(w) {
        if (!w.resourceClass ||
            String(w.resourceClass).toLowerCase().indexOf("premiere") === -1) return;
        w.captionChanged.connect(function () { place(w); });
        w.frameGeometryChanged.connect(function () { place(w); });
        place(w);
    }

    workspace.windowAdded.connect(hook);
    var existing = workspace.windowList();
    for (var i = 0; i < existing.length; i++) hook(existing[i]);

    log("loaded (CSD, offset=" + OFFSET + ")");
})();
