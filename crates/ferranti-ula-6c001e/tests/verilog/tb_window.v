`timescale 1ns / 1ps

// Measure, from the real Verilog, the T-state after /INT at which the
// contention window first stalls the CPU clock.
//
// The address is held at $4000 — A14 high, A15 low, so the memory decode
// is satisfied — and /MREQ is held inactive so `mreqt23` stays high and
// Nor1 reduces to the raster terms alone. Whatever stalls CPUClk is then
// the contention window and nothing else.
module tb_window;
    reg clk14 = 0;
    always #1 clk14 = ~clk14;

    reg [15:0] a = 16'h4000;
    reg [7:0]  din = 8'h00;
    reg mreq_n = 1'b1;
    reg iorq_n = 1'b1;
    reg rd_n   = 1'b1;
    reg wr_n   = 1'b1;
    reg rfsh_n = 1'b1;
    reg ear = 1'b0;
    reg [4:0] kbcolumns = 5'b11111;
    reg [7:0] vramdout = 8'h00;

    wire [7:0] dout, vramdin;
    wire [13:0] va;
    wire clkcpu, msk_int_n, vramoe, vramcs, vramwe;
    wire mic, spk, r, g, b, i, csync;
    wire [7:0] kbrows;

    ula dut(
        .clk14(clk14), .a(a), .din(din), .dout(dout),
        .mreq_n(mreq_n), .iorq_n(iorq_n), .rd_n(rd_n), .wr_n(wr_n),
        .rfsh_n(rfsh_n), .clkcpu(clkcpu), .msk_int_n(msk_int_n),
        .va(va), .vramdout(vramdout), .vramdin(vramdin),
        .vramoe(vramoe), .vramcs(vramcs), .vramwe(vramwe),
        .ear(ear), .mic(mic), .spk(spk), .kbrows(kbrows),
        .kbcolumns(kbcolumns), .r(r), .g(g), .b(b), .i(i), .csync(csync)
    );

    integer ticks = 0;        // clk14 since /INT fell
    integer high_run = 0;     // consecutive clk14 with CPUClk high
    integer reported = 0;
    reg armed = 0;
    reg seen_int = 0;
    reg prev_int = 1;

    // One T-state is four clk14: CPUClk = clk7/2 = clk14/4.
    always @(posedge clk14) begin
        if (prev_int && !msk_int_n && !seen_int) begin
            seen_int <= 1;
            ticks <= 0;
            high_run <= 0;
            $display("INT asserted");
        end else if (seen_int) begin
            ticks <= ticks + 1;
            if (clkcpu) high_run <= high_run + 1;
            else high_run <= 0;

            // A free-running CPUClk is high for exactly two clk14. Longer
            // means the gate is holding it — a stall.
            if (clkcpu && high_run == 2 && reported < 8) begin
                $display("stall starts: clk14=%0d  T-state=%0d.%0d",
                         ticks, ticks / 4, ticks % 4);
                reported <= reported + 1;
            end
            if (ticks > 260000) $finish;
        end
        prev_int <= msk_int_n;
    end
endmodule
