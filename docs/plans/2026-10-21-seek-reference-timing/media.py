# SPDX-License-Identifier: GPL-2.0-or-later
# Reused authored media fixture from the DTL-zero study.
PAYLOAD = bytes((17 * i + 3) & 255 for i in range(128))


def edsk(deleted: bool, bad_crc: bool) -> bytes:
    data = bytearray(768)
    signature = b"EXTENDED CPC DSK File\r\nDisk-Info\r\n"
    data[: len(signature)] = signature
    data[0x22:0x30] = b"198x DTL zero "
    data[0x30:0x32] = bytes([1, 1])
    data[0x34] = 2
    data[256:268] = b"Track-Info\r\n"
    data[256 + 0x13] = 2
    data[256 + 0x14 : 256 + 0x18] = bytes([0, 1, 0x2A, 0xE5])
    data[280:288] = bytes(
        [
            0,
            0,
            1,
            0,
            0x20 if bad_crc else 0,
            (0x40 if deleted else 0) | (0x20 if bad_crc else 0),
            128,
            0,
        ]
    )
    data[512:640] = PAYLOAD
    return bytes(data)
