import argparse
import gzip
import hashlib
import json
import subprocess
from pathlib import Path

parser = argparse.ArgumentParser(
    description="Compare multiword DMA startup in retained WinUAE/vAmiga methods."
)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
corpus = Path(__file__).resolve().parents[1]
pins = json.loads(Path(__file__).with_name("inputs.json").read_text())
for name, digest in pins.items():
    if hashlib.sha256((corpus / name).read_bytes()).hexdigest() != digest:
        raise ValueError(f"reference input changed: {name}")
out = args.output.resolve()
out.mkdir(parents=True, exist_ok=True)
base = gzip.decompress(
    (corpus / "interrupt-probe/reference.cpp.gz").read_bytes()
).decode()
base = base[: base.index("template<isize nr> StateMachine<nr>& channel")]
main = r"""
int main() {
  for(unsigned len:{1,2,8,64}) for(unsigned per:{124,424}) {
    Paula p; auto &ch=p.channel0;
    ch.dma=true; ch.audlenLatch=len; ch.audperLatch=per;
    ch.move_000_001();
    unsigned seen=0;
    for(unsigned t=0;t<250000 && seen<4;++t) {
      p.clock=ch.agnus.clock=t*8;
      p.deliver_irqs();
      if(t%227==14 && ch.audDR) {
        ch.audDR=false; ch.pokeAUDxDAT(0x1122);
      }
      if(ch.agnus.due==ch.agnus.clock) ch.serviceEvent();
      if(p.intreq&128) { std::cout<<len<<','<<per<<','<<seen++<<','<<t<<'\n'; p.intreq&=~128; }
    }
    if(seen!=4) return 2;
  }
}
"""
(out / "reference.cpp").write_text(base + main)
subprocess.run(
    [
        "c++",
        "-std=c++20",
        "-O2",
        str(out / "reference.cpp"),
        "-o",
        str(out / "reference"),
    ],
    check=True,
)
(out / "reference.csv").write_text(
    subprocess.check_output([str(out / "reference")], text=True)
)
uae = gzip.decompress((corpus / "handover-probe/winuae.cpp.gz").read_bytes()).decode()
uae = uae[: uae.index("void run(unsigned nr")]
uae += r"""
int main() {
  for(unsigned len:{1,2,8,64}) for(unsigned per:{124,424}) {
    for(auto &ch:audio_channel) ch=audio_channel_data{};
    for(auto &due:irq_due) due=MAX_EV;
    now=0; intreq=0; dmacon=DMA_MASTER|1; adkcon=0;
    auto &ch=audio_channel[0]; ch.per=per; ch.len=ch.wlen=len;
    service(0,false);
    unsigned seen=0;
    for(now=0;now<250000 && seen<4;++now) {
      deliver_irqs();
      if(ch.evtime!=MAX_EV) { assert(ch.evtime>0); --ch.evtime; }
      if(now%227==14 && ch.dr) {
        ch.dr=false; ch.dsr=false; dat_write(0,0x1122);
      }
      if(ch.evtime==0) service(0,true);
      if(intreq&128) { std::cout<<len<<','<<per<<','<<seen++<<','<<now<<'\n'; intreq&=~128; }
    }
    if(seen!=4) return 2;
  }
}
"""
(out / "winuae.cpp").write_text(uae)
subprocess.run(
    ["c++", "-std=c++20", "-O2", str(out / "winuae.cpp"), "-o", str(out / "winuae")],
    check=True,
)
(out / "winuae.csv").write_text(
    subprocess.check_output([str(out / "winuae")], text=True)
)
assert (out / "winuae.csv").read_text() == (out / "reference.csv").read_text(), (
    "independent references disagree"
)
rows = (out / "reference.csv").read_text().splitlines()
assert len(rows) == 32 and len(set(rows)) == 32
print("WinUAE and vAmiga agree on all 32 request times (8 startup scenarios).")
