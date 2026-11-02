# Menubar

The menu is the menubar panel. Level 0 shows Core state, then Settings, About, and Quit. The Core row opens a submenu of modules and recent activity. Submenus cascade to depth 3, each beside its parent. Opening the Core submenu is budgeted at 16 ms.

The glass tint is black at 69% in dark appearance and white at 57% in light.

The menu preview is dev-only. It shows sample rows and the tint slider, and it does not call `setPanelVisible`.
