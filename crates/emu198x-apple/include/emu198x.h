/*
 * C interface to the Emu198x runtime catalogue, for native Apple hosts.
 *
 * Each function forwards to the matching method of `Fleet` in
 * crates/emu198x-fleet-web, so a native host runs the same emulation as
 * the browser player. tests/header.rs checks that this file declares every
 * exported function and the same status values as src/lib.rs.
 *
 * Caller obligations:
 *   - Pointers are null or valid for the length the call states.
 *   - Strings are NUL-terminated UTF-8.
 *   - One machine is used from one thread at a time.
 *
 * Every function except the two free functions and emu198x_last_error
 * returns a status. After a failure, emu198x_last_error() describes it.
 * No call lets a Rust panic escape: a panic returns EMU198X_PANICKED and
 * poisons the machine, which then returns EMU198X_POISONED until freed.
 */
#ifndef EMU198X_H
#define EMU198X_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define EMU198X_OK 0
#define EMU198X_ERROR 1
#define EMU198X_INVALID_ARGUMENT 2
#define EMU198X_PANICKED 3
#define EMU198X_POISONED 4

typedef struct Emu198xMachine Emu198xMachine;

/* Bytes owned by the library. Release with emu198x_buffer_free. */
typedef struct Emu198xBuffer {
    uint8_t *data;
    size_t len;
} Emu198xBuffer;

/* The compiled families and variants as UTF-8 JSON (not NUL-terminated). */
int32_t emu198x_catalogue(Emu198xBuffer *out);

/* Creates an unbooted machine. *out is null on failure. */
int32_t emu198x_machine_create(const char *family, const char *variant, Emu198xMachine **out);
/* Frees a machine. Null is ignored. */
void emu198x_machine_free(Emu198xMachine *machine);

/* Supplies a firmware image. Only before boot. */
int32_t emu198x_machine_firmware(Emu198xMachine *machine, const char *id, const uint8_t *data, size_t len);
/* Builds the runtime from the supplied firmware. */
int32_t emu198x_machine_boot(Emu198xMachine *machine);

/* Advances by wall-clock time; *frames_out is how many frames ran. */
int32_t emu198x_machine_advance(Emu198xMachine *machine, double elapsed_ms, uint32_t *frames_out);
/* Runs exactly one frame, ignoring the clock. */
int32_t emu198x_machine_step(Emu198xMachine *machine);
/* The machine's frame period in milliseconds. */
int32_t emu198x_machine_frame_ms(Emu198xMachine *machine, double *out);

/* Loads media into a declared slot, such as "tape-1" or "cartridge". */
int32_t emu198x_machine_load(Emu198xMachine *machine, const char *slot, const uint8_t *data, size_t len);
/* Starts or stops a tape deck. */
int32_t emu198x_machine_transport(Emu198xMachine *machine, const char *slot, bool playing);

/* Loads the inserted tape the way a person would from power-on: reset,
 * wait for the prompt, type LOAD "" and press play. *boot_frames_out is the
 * frames spent booting. Spectrum only; other machines are refused. The tape
 * then loads at normal speed as the machine runs. */
int32_t emu198x_machine_autoload_tape(Emu198xMachine *machine, uint32_t max_boot_frames, uint32_t *boot_frames_out);

/* Presses or releases a machine key by name. */
int32_t emu198x_machine_key(Emu198xMachine *machine, const char *name, bool pressed);
/* Presses or releases a named control on a numbered input port. Ports are
 * the numbers printed on the machine; 0 is its primary joystick (Kempston
 * on a Spectrum) and 1 is controller one on a console. */
int32_t emu198x_machine_button(Emu198xMachine *machine, uint8_t port, const char *name, bool pressed);
/* Browser key code to machine-key map as UTF-8 JSON. */
int32_t emu198x_machine_keymap(Emu198xMachine *machine, Emu198xBuffer *out);
/* Answers a runtime query path, such as "tape.playing", as UTF-8 JSON. */
int32_t emu198x_machine_query(Emu198xMachine *machine, const char *path, Emu198xBuffer *out);

/* Turns on interleaved stereo sound at rate samples per second. */
int32_t emu198x_machine_configure_audio(Emu198xMachine *machine, uint32_t rate);
/* Copies up to capacity samples, in whole stereo pairs. Samples that do
 * not fit stay queued, up to one second; older ones are dropped. */
int32_t emu198x_machine_audio(Emu198xMachine *machine, float *out, size_t capacity, size_t *written_out);

/* The picture size in pixels. */
int32_t emu198x_machine_frame_size(Emu198xMachine *machine, uint32_t *width_out, uint32_t *height_out);
/* Width of one picture pixel relative to its height, from the runtime's
 * statement of its display: about 1.055 for a PAL Spectrum, 1 for an LCD. */
int32_t emu198x_machine_pixel_aspect(Emu198xMachine *machine, float *out);
/* Copies the latest frame as RGBA, width * height * 4 bytes. A smaller
 * buffer is refused with EMU198X_INVALID_ARGUMENT. */
int32_t emu198x_machine_pixels(Emu198xMachine *machine, uint8_t *out, size_t capacity, size_t *written_out);

/* Exports and restores the complete machine state. */
int32_t emu198x_machine_save_state(Emu198xMachine *machine, Emu198xBuffer *out);
int32_t emu198x_machine_restore_state(Emu198xMachine *machine, const uint8_t *data, size_t len);

/* Releases a buffer this library returned. */
void emu198x_buffer_free(Emu198xBuffer buffer);

/* The calling thread's last error, or null after a successful call.
 * Valid until the next call on the same thread. */
const char *emu198x_last_error(void);

#ifdef __cplusplus
}
#endif

#endif /* EMU198X_H */
