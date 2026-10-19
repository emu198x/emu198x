/* SPDX-License-Identifier: GPL-2.0-or-later */
/* Standalone host: queued events in absolute T-states; no Scan/drive logic. */
#include "config.h"
#include "settings.h"
#include "event.h"
#include <stdarg.h>
struct probe_machine machine = {{3546900}};
struct probe_machine *machine_current = &machine;
struct probe_settings settings_current = {40, 80};
libspectrum_dword tstates;
static event_fn_t callbacks[16];
static int callback_count;
static struct { int active, type; libspectrum_dword when; void *data; } queue[128];
int event_register(event_fn_t fn, const char *description) {
    (void)description;
    assert(callback_count < 16);
    callbacks[callback_count] = fn;
    return callback_count++;
}
void event_add_with_data(libspectrum_dword when, int type, void *data) {
    assert(type >= 0 && type < callback_count && when >= tstates);
    for (size_t i = 0; i < ARRAY_SIZE(queue); i++) if (!queue[i].active) {
        queue[i].active = 1; queue[i].type = type; queue[i].when = when; queue[i].data = data; return;
    }
    fprintf(stderr, "event queue bound\n"); exit(2);
}
void event_remove_type(int type) {
    for (size_t i = 0; i < ARRAY_SIZE(queue); i++)
        if (queue[i].type == type) queue[i].active = 0;
}
void event_remove_type_user_data(int type, void *data) {
    for (size_t i = 0; i < ARRAY_SIZE(queue); i++)
        if (queue[i].type == type && queue[i].data == data) queue[i].active = 0;
}
void probe_advance(void) {
    tstates++;
    for (unsigned dispatched = 0; dispatched < 128; dispatched++) {
        int chosen = -1;
        for (size_t i = 0; i < ARRAY_SIZE(queue); i++)
            if (queue[i].active && queue[i].when <= tstates &&
                (chosen < 0 || queue[i].when < queue[chosen].when)) chosen = (int)i;
        if (chosen < 0) return;
        libspectrum_dword when = queue[chosen].when;
        int type = queue[chosen].type; void *data = queue[chosen].data;
        queue[chosen].active = 0; callbacks[type](when, type, data);
    }
    fprintf(stderr, "same-tick event bound\n"); exit(2);
}
void wd_fdc_init_events(void) {}
int ui_error(int severity, const char *format, ...) {
    (void)severity; va_list args; va_start(args, format); vfprintf(stderr, format, args); va_end(args); return 0;
}
void ui_statusbar_update(int item, int state) { (void)item; (void)state; }
void startup_manager_register(int module, const int *dependencies, size_t count,
                             int (*init)(void *), void *a, void *b) {
    (void)module; (void)dependencies; (void)count; (void)a; (void)b;
    assert(init(NULL) == 0);
}
