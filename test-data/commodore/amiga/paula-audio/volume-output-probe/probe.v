// Exercise the unchanged pinned channel through register pins only.
module probe;
  reg clk=0, clk7_en=1, cck=0, reset=0, aen=0, dmaena=0;
  reg [2:0] reg_address_in=0;
  reg [15:0] data=0;
  wire [6:0] volume;
  wire [7:0] sample, sample_okk;
  wire intreq, dmareq, dmas;
  paula_audio_channel dut(
    .clk(clk), .clk7_en(clk7_en), .cck(cck), .reset(reset),
    .aen(aen), .dmaena(dmaena), .reg_address_in(reg_address_in),
    .data(data), .volume(volume), .sample(sample), .sample_okk(sample_okk),
    .intreq(intreq), .intpen(1'b0), .dmareq(dmareq), .dmas(dmas),
    .strhor(1'b0)
  );
  task edge7;
    begin #5; clk=1; #5; clk=0; end
  endtask
  task tick;
    begin cck=1; edge7; cck=0; end
  endtask
  task write_reg(input [2:0] addr, input [15:0] value);
    begin aen=1; reg_address_in=addr; data=value; edge7; aen=0; end
  endtask
  task start(input integer initial_volume);
    integer tries;
    begin
      reset=1; tick; reset=0;
      write_reg(3, 1000); // AUDPER: keep sample boundaries outside observation.
      write_reg(4, initial_volume);
      write_reg(5, 16'h4040);
      tries=0;
      while (dut.audio_state !== dut.AUDIO_STATE_3 && tries<8) begin tick; tries=tries+1; end
      if (dut.audio_state !== dut.AUDIO_STATE_3) $fatal(1, "manual startup did not reach high byte");
    end
  endtask
  task run(input integer dynamic_write, input integer value, input integer phase);
    integer t;
    begin
      start(dynamic_write ? 32 : value);
      for(t=0;t<64;t=t+1) begin
        if(dynamic_write && t==phase) write_reg(4, value);
        $display("%0d,%0d,%0d,%0d,%0d,%0d,%0d,%0d",
          dynamic_write,value,phase,t,dut.volcnt,volume,sample,sample_okk);
        tick;
      end
    end
  endtask
  integer value, phase;
  initial begin
    for(value=0;value<128;value=value+1) run(0,value,0);
    for(phase=0;phase<64;phase=phase+1) begin
      run(1,0,phase); run(1,1,phase); run(1,31,phase); run(1,32,phase);
      run(1,63,phase); run(1,64,phase); run(1,127,phase);
    end
    $finish;
  end
endmodule
