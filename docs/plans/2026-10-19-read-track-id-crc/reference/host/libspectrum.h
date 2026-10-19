/* SPDX-License-Identifier: GPL-2.0-or-later */
#ifndef PROBE_LIBSPECTRUM_H
#define PROBE_LIBSPECTRUM_H
#include <stdint.h>
#include <stdlib.h>
typedef uint8_t libspectrum_byte;
typedef uint16_t libspectrum_word;
typedef uint32_t libspectrum_dword;
typedef int32_t libspectrum_signed_dword;
#define libspectrum_new(type, count) ((type *)calloc((count), sizeof(type)))
typedef void *gpointer;
typedef void (*GFunc)(void *, void *);
#endif
