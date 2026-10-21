/* SPDX-License-Identifier: GPL-2.0-or-later */
#ifndef PROBE_SETTINGS_H
#define PROBE_SETTINGS_H
struct probe_settings { int drive_40_max_track, drive_80_max_track; };
extern struct probe_settings settings_current;
#endif
