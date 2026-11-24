// Run with osascript -l JavaScript and the PID being checked for a startup modal.
ObjC.import("ApplicationServices");

function read(element, attribute) {
    const value = Ref();
    if ($.AXUIElementCopyAttributeValue(element, $(attribute), value) === 0) {
        return ObjC.unwrap(ObjC.castRefToObject(value[0]));
    }
}

function run([pid]) {
    if (!$.AXIsProcessTrusted()) return "unavailable";

    const app = $.AXUIElementCreateApplication(Number(pid));
    $.AXUIElementSetMessagingTimeout(app, 1);
    for (const window of read(app, "AXChildren") || []) {
        if (read(window, "AXTitle") !== "Warning") continue;

        const controls = read(window, "AXChildren") || [];
        const warning = controls.some(control => {
            const message = read(control, "AXDescription") || "";
            return message.startsWith("Your disk cache folder") && message.includes("enough available space");
        });
        if (!warning) continue;

        const once = controls.find(control => read(control, "AXRole") === "AXRadioButton" &&
            read(control, "AXDescription") === "Once Per Session");
        const ok = controls.find(control => read(control, "AXRole") === "AXButton" &&
            read(control, "AXDescription") === "OK");
        if (!once || read(once, "AXValue") !== true || !ok) return "blocked";
        return $.AXUIElementPerformAction(ok, $("AXPress")) === 0 ? "dismissed" : "blocked";
    }
    return "absent";
}
