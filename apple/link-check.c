/*
 * Links against Emu198xCore and calls it, the way the Swift host will.
 *
 * scripts/build-apple-xcframework.sh links this against every slice, which
 * proves each library resolves with Apple's toolchain, and runs the macOS
 * build. Exit status 0 means every check below held.
 */
#include <pthread.h>
#include <stdio.h>
#include <string.h>

#include "emu198x.h"

static int fail(const char *what) {
    const char *error = emu198x_last_error();
    fprintf(stderr, "link-check: %s (%s)\n", what, error ? error : "no message");
    return 1;
}

static int check_machine(void) {
    Emu198xBuffer catalogue = {0};
    if (emu198x_catalogue(&catalogue) != EMU198X_OK || catalogue.len < 2) {
        return fail("catalogue");
    }
    emu198x_buffer_free(catalogue);

    Emu198xBuffer commands = {0};
    if (emu198x_commands(&commands) != EMU198X_OK || commands.len < 2) {
        return fail("commands");
    }
    emu198x_buffer_free(commands);

    Emu198xMachine *machine = NULL;
    if (emu198x_machine_create("sinclair-zx-spectrum", "spectrum_48k", &machine) != EMU198X_OK) {
        return fail("create a Spectrum 48K");
    }
    /* No firmware was supplied, so boot must refuse and say why. */
    int32_t code = emu198x_machine_boot(machine);
    const char *error = emu198x_last_error();
    if (code != EMU198X_ERROR || error == NULL || strstr(error, "missing") == NULL) {
        emu198x_machine_free(machine);
        return fail("boot without firmware was not refused");
    }
    /* Swift Testing and the host may call from a 512 KiB worker stack.
     * Exercise the actual constructor and save decoder there, not just a
     * missing-firmware refusal that exits before constructing a board. */
    uint8_t rom[16384] = {0};
    rom[0] = 0x18; /* JR $: keep the synthetic firmware in ROM. */
    rom[1] = 0xfe;
    if (emu198x_machine_firmware(machine, "sinclair-zx-spectrum-48k-rom", rom, sizeof rom) != EMU198X_OK ||
        emu198x_machine_boot(machine) != EMU198X_OK ||
        emu198x_machine_step(machine) != EMU198X_OK) {
        emu198x_machine_free(machine);
        return fail("boot and step synthetic firmware");
    }
    Emu198xBuffer state = {0};
    if (emu198x_machine_save_state(machine, &state) != EMU198X_OK) {
        emu198x_machine_free(machine);
        return fail("save synthetic machine");
    }
    code = emu198x_machine_restore_state(machine, state.data, state.len);
    emu198x_buffer_free(state);
    emu198x_machine_free(machine);
    if (code != EMU198X_OK) {
        return fail("restore synthetic machine");
    }
    puts("link-check: ok (512 KiB worker stack)");
    return 0;
}

static void *check_on_worker(void *result) {
    *(int *)result = check_machine();
    return NULL;
}

int main(void) {
    pthread_attr_t attributes;
    if (pthread_attr_init(&attributes) != 0) return fail("thread attributes");
    if (pthread_attr_setstacksize(&attributes, 512 * 1024) != 0) {
        pthread_attr_destroy(&attributes);
        return fail("worker stack size");
    }
    pthread_t worker;
    int result = 1;
    int created = pthread_create(&worker, &attributes, check_on_worker, &result);
    pthread_attr_destroy(&attributes);
    if (created != 0) return fail("create worker");
    if (pthread_join(worker, NULL) != 0) return fail("join worker");
    return result;
}
