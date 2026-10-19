/* SPDX-License-Identifier: GPL-2.0-or-later */
#ifndef PROBE_CONFIG_H
#define PROBE_CONFIG_H
#ifdef NDEBUG
#error "This probe requires active assertions"
#endif
/* Replace host-only headers, never the controller/drive/media implementation. */
#define FUSE_FUSE_H
#define FUSE_UI_H
#define FUSE_MACHINE_H
#define FUSE_SPECTRUM_H
#define FUSE_COMPAT_H
#define FUSE_STARTUP_MANAGER_H
#define FUSE_WD_FDC_H
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "libspectrum.h"
#define GCC_UNUSED __attribute__((unused))
#define ARRAY_SIZE(a) (sizeof(a) / sizeof((a)[0]))
struct probe_machine { struct { unsigned processor_speed; } timings; };
extern struct probe_machine *machine_current;
extern libspectrum_dword tstates;
void probe_advance(void);
void wd_fdc_init_events(void);
int ui_error(int severity, const char *format, ...);
void ui_statusbar_update(int item, int state);
enum { UI_ERROR_WARNING, UI_STATUSBAR_ITEM_DISK, UI_STATUSBAR_STATE_ACTIVE, UI_STATUSBAR_STATE_INACTIVE };
typedef int startup_manager_module;
enum { STARTUP_MANAGER_MODULE_EVENT, STARTUP_MANAGER_MODULE_SETUID, STARTUP_MANAGER_MODULE_FDD };
void startup_manager_register(int module, const int *dependencies, size_t count,
                             int (*init)(void *), void *a, void *b);
#endif
