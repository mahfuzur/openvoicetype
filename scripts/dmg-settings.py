# dmgbuild settings for the release DMG: a 660 x 400 window with the background from scripts/make-artwork.swift,
# the app on the left and an Applications link on the right, no toolbar or sidebar. Used by scripts/release.sh:
#   dmgbuild -s scripts/dmg-settings.py -D app=<VoiceToText.app> -D background=<tiff> -D icon=<icns> "OpenVoiceType" out.dmg
# `defines` is provided by dmgbuild.
app = defines["app"]  # noqa: F821
# Finder shows the file name under the icon, and it's what lands in Applications: the product name, not "VoiceToText".
app_name = "OpenVoiceType.app"

files = [(app, app_name)]
symlinks = {"Applications": "/Applications"}
icon = defines["icon"]  # noqa: F821  (the volume icon, shown on the desktop and in Finder's sidebar)
background = defines["background"]  # noqa: F821

format = "UDZO"
filesystem = "HFS+"

default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
show_icon_preview = False

# The window's content matches the background (660 x 400 points). Icon positions are their centres.
window_rect = ((200, 140), (660, 400))
icon_size = 128
text_size = 13
arrange_by = None
icon_locations = {
    app_name: (165, 190),
    "Applications": (495, 190),
}
