-- Run with MAME's aes driver, the mslug software and Asia/Japan BIOS.
-- Set EMU198X_REFERENCE_CAPTURE to a new output PNG path.
local output = assert(os.getenv("EMU198X_REFERENCE_CAPTURE"), "capture path required")
local captured = false
emu.register_frame_done(function()
  local screen = manager.machine.screens[":screen"]
  if not captured and screen:frame_number() >= 900 then
    assert(screen:frame_number() == 900, "capture missed the requested frame")
    captured = true
    local err = screen:snapshot(output)
    assert(err == nil, tostring(err))
    local cpu = manager.machine.devices[":maincpu"]
    local space = cpu.spaces["program"]
    assert(space:read_u32(0x100) == 0x4E454F2D, "cartridge recognition header")
    assert(space:read_u16(0x108) == 0x0201, "Metal Slug game ID")
    print(string.format("REFERENCE pc=%08X header=%08X game_id=%04X frames=%d", cpu.state["PC"].value, space:read_u32(0x100), space:read_u16(0x108), screen:frame_number()))
    manager.machine:exit()
  end
end, "frame")
