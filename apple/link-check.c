/*
 * Links against Emu198xCore and calls it, the way the Swift host will.
 *
 * scripts/build-apple-xcframework.sh links this against every slice, which
 * proves each library resolves with Apple's toolchain, and runs the macOS
 * build. Exit status 0 means every check below held.
 */
#include <stdio.h>
#include <string.h>

#include "emu198x.h"

static int fail(const char *what) {
    const char *error = emu198x_last_error();
    fprintf(stderr, "link-check: %s (%s)\n", what, error ? error : "no message");
    return 1;
}

int main(void) {
    Emu198xBuffer catalogue = {0};
    if (emu198x_catalogue(&catalogue) != EMU198X_OK || catalogue.len < 2) {
        return fail("catalogue");
    }
    emu198x_buffer_free(catalogue);

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
    emu198x_machine_free(machine);
    puts("link-check: ok");
    return 0;
}
