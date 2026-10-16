ASM ?= asm198x
BUILD ?= build198x

all: demo.adf

# Asm198x's vasm dialect emits the hunk executable AmigaDOS loads.
demo: demo.asm paula-sample.inc
	$(ASM) --dialect vasm --exe demo.asm -o $@

# Build198x masters a bootable OFS floppy that runs the executable.
demo.adf: demo
	$(BUILD) adf demo -o $@ --volume Sample --name demo

clean:
	rm -f demo demo.adf

.PHONY: all clean
