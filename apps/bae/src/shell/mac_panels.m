// AppKit runs these modal interactions on the shell's main thread.
#import <AppKit/AppKit.h>
#import <UserNotifications/UserNotifications.h>

@interface BAEImportOptions : NSObject <NSOpenSavePanelDelegate>
@property(strong) NSButton *sequence;
@property(strong) NSButton *order;
@property(strong) NSButton *create;
@property(strong) NSPopUpButton *kind;
@end

@implementation BAEImportOptions
- (void)updateOrder {
    self.order.enabled = self.sequence.enabled && self.sequence.state == NSControlStateValueOn;
}

- (void)panelSelectionDidChange:(NSOpenPanel *)panel {
    NSArray *images = @[@"png", @"jpg", @"jpeg", @"jpe", @"jfif", @"tif", @"tiff", @"exr", @"webp"];
    NSString *extension = panel.URL.pathExtension.lowercaseString;
    BOOL layered = NO;
    BOOL ordinary = NO;
    for (NSURL *url in panel.URLs) {
        NSString *ext = url.pathExtension.lowercaseString;
        if ([ext isEqualToString:@"psd"] || [ext isEqualToString:@"psb"]) {
            layered = YES;
        } else {
            ordinary = YES;
        }
    }
    self.create.enabled = !layered || ordinary;
    if (!self.create.enabled) self.create.state = NSControlStateValueOff;
    self.kind.enabled = layered;
    if (!layered) [self.kind selectItemAtIndex:0];
    self.sequence.enabled = [images containsObject:extension ?: @""];
    NSString *stem = panel.URL.lastPathComponent.stringByDeletingPathExtension;
    BOOL numbered = stem && [stem rangeOfCharacterFromSet:NSCharacterSet.decimalDigitCharacterSet].location != NSNotFound;
    self.sequence.state = numbered && self.sequence.enabled ? NSControlStateValueOn : NSControlStateValueOff;
    [self updateOrder];
}
@end

NSArray<NSURL *> *bae_import_panel(NSView *owner, NSArray<NSString *> *labels,
                                   bool *sequences, bool *alphabetical, bool *createComp, NSInteger *importKind) NS_RETURNS_RETAINED;
NSArray<NSURL *> *bae_import_panel(NSView *owner, NSArray<NSString *> *labels,
                                   bool *sequences, bool *alphabetical, bool *createComp, NSInteger *importKind) {
    NSOpenPanel *panel = NSOpenPanel.openPanel;
    panel.title = labels[0];
    panel.canChooseDirectories = YES;
    panel.canChooseFiles = YES;
    panel.allowsMultipleSelection = YES;
    NSButton *sequence = [NSButton checkboxWithTitle:labels[1] target:nil action:nil];
    sequence.state = NSControlStateValueOn;
    NSButton *create = [NSButton checkboxWithTitle:labels[2] target:nil action:nil];
    NSButton *order = [NSButton checkboxWithTitle:labels[3] target:nil action:nil];
    __attribute__((objc_precise_lifetime)) BAEImportOptions *delegate = [BAEImportOptions new];
    delegate.sequence = sequence;
    delegate.order = order;
    delegate.create = create;
    sequence.target = delegate;
    sequence.action = @selector(updateOrder);
    panel.delegate = delegate;
    NSPopUpButton *kind = [NSPopUpButton new];
    [kind addItemsWithTitles:@[labels[5], labels[6], labels[7]]];
    delegate.kind = kind;
    [delegate panelSelectionDidChange:panel];
    NSStackView *kindRow = [NSStackView stackViewWithViews:@[[NSTextField labelWithString:labels[4]], kind]];
    NSStackView *options = [NSStackView stackViewWithViews:@[kindRow, sequence, order, create]];
    options.orientation = NSUserInterfaceLayoutOrientationVertical;
    options.alignment = NSLayoutAttributeLeading;
    options.edgeInsets = NSEdgeInsetsMake(6, 12, 6, 12);
    panel.accessoryView = options;
    panel.accessoryViewDisclosed = YES;
    if (owner.window) {
        [panel beginSheetModalForWindow:owner.window completionHandler:^(NSModalResponse result) { (void)result; }];
    }
    NSModalResponse result = [panel runModal];
    panel.delegate = nil;
    (void)delegate;
    if (result != NSModalResponseOK) return nil;
    *sequences = sequence.state == NSControlStateValueOn;
    *alphabetical = *sequences && order.state == NSControlStateValueOn;
    *createComp = create.state == NSControlStateValueOn;
    *importKind = kind.indexOfSelectedItem;
    return panel.URLs;
}

NSInteger bae_confirm_close(NSView *owner, NSArray<NSString *> *labels) {
    NSAlert *alert = [NSAlert new];
    alert.messageText = labels[0];
    alert.informativeText = labels[1];
    [alert addButtonWithTitle:labels[2]].keyEquivalent = @"\r";
    [alert addButtonWithTitle:labels[3]].keyEquivalent = @"\033";
    NSButton *discard = [alert addButtonWithTitle:labels[4]];
    discard.keyEquivalent = @"d";
    discard.keyEquivalentModifierMask = NSEventModifierFlagCommand;
    if (@available(macOS 11, *)) discard.hasDestructiveAction = YES;
    if (owner.window) {
        [alert beginSheetModalForWindow:owner.window completionHandler:^(NSModalResponse result) {
            [NSApp stopModalWithCode:result];
        }];
        return [NSApp runModalForWindow:alert.window] - NSAlertFirstButtonReturn;
    }
    return [alert runModal] - NSAlertFirstButtonReturn;
}

void bae_dock_progress(int percent) {
    NSApp.dockTile.badgeLabel = percent < 0 ? nil : [NSString stringWithFormat:@"%d%%", percent];
}

@interface BAERenderNotifications : NSObject <UNUserNotificationCenterDelegate>
@end
@implementation BAERenderNotifications
- (void)userNotificationCenter:(UNUserNotificationCenter *)center
       willPresentNotification:(UNNotification *)notification
         withCompletionHandler:(void (^)(UNNotificationPresentationOptions))completion {
    (void)center;
    (void)notification;
    completion(UNNotificationPresentationOptionBanner | UNNotificationPresentationOptionList);
}
@end

// Called by the shared low-priority warm-up thread. Never requests permission.
void bae_prepare_notifications(void) {
    @autoreleasepool {
        if (!NSBundle.mainBundle.bundleIdentifier) return;
        static BAERenderNotifications *delegate;
        static dispatch_once_t once;
        dispatch_once(&once, ^{
            delegate = [BAERenderNotifications new];
            UNUserNotificationCenter.currentNotificationCenter.delegate = delegate;
        });
    }
}

void bae_render_notification(NSString *title) {
    // Command-line and test executables have no notification identity.
    if (!NSBundle.mainBundle.bundleIdentifier) return;
    UNUserNotificationCenter *center = UNUserNotificationCenter.currentNotificationCenter;
    [center requestAuthorizationWithOptions:UNAuthorizationOptionAlert
                          completionHandler:^(BOOL allowed, NSError *error) {
        if (error) NSLog(@"Render notification authorization: %@", error.localizedDescription);
        if (!allowed) return;
        UNMutableNotificationContent *content = [UNMutableNotificationContent new];
        content.title = title;
        UNNotificationRequest *request = [UNNotificationRequest
            requestWithIdentifier:NSUUID.UUID.UUIDString content:content trigger:nil];
        [center addNotificationRequest:request withCompletionHandler:^(NSError *err) {
            if (err) NSLog(@"Render notification: %@", err.localizedDescription);
        }];
    }];
}

void bae_system_shortcuts(bool standard) {
    // Muda's system-owned Hide items target NSApplication directly.
    for (NSMenuItem *item in NSApp.mainMenu.itemArray.firstObject.submenu.itemArray) {
        if (item.action == @selector(hide:) || item.action == @selector(hideOtherApplications:)) {
            NSEventModifierFlags mods = NSEventModifierFlagCommand;
            if (item.action == @selector(hideOtherApplications:)) mods |= NSEventModifierFlagOption;
            if (!standard) mods |= NSEventModifierFlagControl;
            item.keyEquivalentModifierMask = mods;
        }
    }
}
