`timescale 1ns / 1ps

// Does a ULA-answered port contend even when its address is outside the
// contended page? That is the claim that resolved the Chapter 18 fork —
// `Nor1`'s IORQ term satisfying both address conditions on its own — and
// it cannot be tested with a statically asserted /IORQ, because
// `ioreqtw3` latches low on the first CPUClk rise and disarms the gate
// permanently. So this pulses /IORQ the way a real I/O cycle does.
module tb_io;
    reg clk14 = 0;
    always #1 clk14 = ~clk14;

    reg [15:0] a = 16'hC0FE;
    reg [7:0]  din = 8'h00;
    reg mreq_n = 1'b1;
    reg iorq_n = 1'b1;
    reg rd_n = 1'b1, wr_n = 1'b1, rfsh_n = 1'b1, ear = 1'b0;
    reg [4:0] kbcolumns = 5'b11111;
    reg [7:0] vramdout = 8'h00;

    wire [7:0] dout, vramdin, kbrows;
    wire [13:0] va;
    wire clkcpu, msk_int_n, vramoe, vramcs, vramwe, mic, spk, r, g, b, i, csync;

    ula dut(.clk14(clk14), .a(a), .din(din), .dout(dout), .mreq_n(mreq_n),
        .iorq_n(iorq_n), .rd_n(rd_n), .wr_n(wr_n), .rfsh_n(rfsh_n),
        .clkcpu(clkcpu), .msk_int_n(msk_int_n), .va(va), .vramdout(vramdout),
        .vramdin(vramdin), .vramoe(vramoe), .vramcs(vramcs), .vramwe(vramwe),
        .ear(ear), .mic(mic), .spk(spk), .kbrows(kbrows),
        .kbcolumns(kbcolumns), .r(r), .g(g), .b(b), .i(i), .csync(csync));

    integer ticks = 0, high_run = 0, stalls = 0;
    reg seen_int = 0, prev_int = 1;

    always @(posedge clk14) begin
        if (prev_int && !msk_int_n && !seen_int) begin
            seen_int <= 1; ticks <= 0;
        end else if (seen_int) begin
            ticks <= ticks + 1;

            // Well inside the display window, pulse /IORQ low for five
            // clk7 (ten clk14) out of every eight T-states — the span a
            // Z80 I/O cycle holds it across T2, TW and T3.
            if (ticks > 57352 && ticks < 57352 + 8960) begin
                if ((ticks % 32) == 4) iorq_n <= 1'b0;
                if ((ticks % 32) == 14) iorq_n <= 1'b1;
            end else begin
                iorq_n <= 1'b1;
            end

            if (clkcpu) high_run <= high_run + 1; else high_run <= 0;
            if (clkcpu && high_run == 2) stalls <= stalls + 1;

            if (ticks == 57352 + 8960) begin
                $display("stalls over 40 display lines: %0d", stalls);
                $finish;
            end
        end
        prev_int <= msk_int_n;
    end
endmodule
