# Original staged-cartridge v5 specimens

Captured before the fix on both boards with two distinct accepted synthetic
128-byte cartridges (17/A5). Each image executes DI, loads its marker, writes
RAM, then halts. ColecoVision images include the AA55 header and start at 8024;
M5 images start at 2000. BIOS had not yet been installed when these saves were
written. Each region's two files are identical because v5 omitted staged bytes.
These immutable specimens qualify the explicit v5 empty reader, which cannot
invent the original cartridge. Their full identities are in sha256.json.
