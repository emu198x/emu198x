# VIC-20 VIC-I VICE survey inputs

This directory identifies the inputs of the VIC-I raster survey against VICE
xvic (#362). The method, the fixture layout and how to re-run it are in
[`knowledge/processes/vic20-vici-vice-survey.md`](../../../../knowledge/processes/vic20-vici-vice-survey.md).

[`cases-v1.json`](cases-v1.json) lists every case: its video standard, RAM
expansion, program, the frame at which both emulators are compared and the
SHA-256 of the program and of VICE's reference capture. It also pins the
VIC-20 ROMs, the sixteen palette-calibration captures, the injection point and
the VICE command line. The survey test refuses any input whose bytes differ.

[`programs/`](programs/) holds the four raster programs this project wrote for
the survey, with their assembled images. Each locks itself to the raster
without the VIAs and then moves a register write one cycle right per line, so
a single frame shows where the VIC-I puts a write made in every cycle of a
line. `programs/build.py --check` confirms the images match their source.

The other inputs stay outside the repository. The staged fixture directory
holds VICE's own VIC-20 test programs (from the VICE Subversion repository at
the revision the manifest records) and the reference captures, which
`scripts/capture-vic20-vici-vice-references.py` regenerates from a local VICE
install. The ROMs come from the usual VIC-20 ROM directory.

Adding a case extends the manifest. Replacing the bytes of anything it already
pins — a program, a reference or a ROM — needs a new manifest version, so that
an old result can always be traced to the inputs that produced it.
