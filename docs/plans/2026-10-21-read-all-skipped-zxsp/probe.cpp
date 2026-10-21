// SPDX-License-Identifier: GPL-2.0-or-later
// Runs byte-identical zxsp Fdc765 sources through public register methods.
#include "Fdc765.h"
#include <fstream>
#include <iostream>
#include <iterator>
#include <string>
#include <vector>
struct Read { unsigned tick, msr, value; };
struct Phase { std::string name; std::vector<unsigned> writes; std::vector<Read> reads; unsigned idle=0; };
class Probe final : public Fdc765 {
    void input(Time,int32,uint16,uint8&,uint8&) override {}
    void output(Time,int32,uint16,uint8) override {}
public:
    explicit Probe(std::shared_ptr<FloppyDiskDrive> d):Fdc765(nullptr,0,true,"","") {
        for(auto& p:fdd) p=d;
        drive=d;
        powerOn(0);
    }
    ~Probe() override=default;
};
int main(int argc,char** argv) {
 try {
    if(argc!=7) throw std::runtime_error("usage: probe track live opcode N EOT DTL");
    auto d=std::make_shared<FloppyDiskDrive>();
    std::ifstream in(argv[1],std::ios::binary);
    if(!in) throw std::runtime_error("missing track");
    d->bytes=std::vector<uint8>(std::istreambuf_iterator<char>(in),{});
    if(d->bytes.size()!=6250) throw std::runtime_error("track must be 6250 decoded bytes");
    Probe f(d); unsigned tick=0;
    auto status=[&]() { if(++tick>8000000) throw std::runtime_error("total emulated limit"); return unsigned(f.readMainStatusRegister(tick*0.000001)); };
    std::vector<Phase> phases;
    auto command=[&](const std::string& name,std::vector<unsigned> bytes) {
        Phase p{name,bytes,{},0};
        const auto limit=tick+2000000;
        for(auto b:bytes) {
            unsigned s;
            do { if(tick>=limit) throw std::runtime_error("command-write emulated limit"); s=status(); } while((s&0xC0)!=0x80);
            f.writeDataRegister(tick*0.000001,uint8(b));
        }
        for(;;) {
            if(tick>=limit) throw std::runtime_error("command-read emulated limit");
            const auto s=status();
            if((s&0xC0)==0xC0) {
                const auto b=f.readDataRegister(tick*0.000001);
                p.reads.push_back({tick,s,b});
                if(p.reads.size()>65536) throw std::runtime_error("read count limit");
            } else if(s==0x80) { p.idle=s; break; }
        }
        phases.push_back(p);
    };
    status(); // let the unchanged controller observe initial READY
    command("initial_sis",{8});
    command("specify",{3,0xDF,3});
    command("read",{unsigned(std::stoul(argv[3],nullptr,0)),0,0,0,1,unsigned(std::stoul(argv[4],nullptr,0)),unsigned(std::stoul(argv[5],nullptr,0)),0x2A,unsigned(std::stoul(argv[6],nullptr,0))});
    command("recovery_sds",{4,0});
    std::ofstream out(argv[2],std::ios::binary); out.write(reinterpret_cast<const char*>(d->bytes.data()),d->bytes.size());
    if(!out) throw std::runtime_error("live media write failed");
    std::cout<<"{\"tick_seconds\":0.000001,\"total_ticks\":"<<tick<<",\"drive_reads\":"<<d->reads<<",\"drive_writes\":"<<d->writes<<",\"phases\":[";
    bool comma=false;
    for(const auto& p:phases) {
        if(comma)std::cout<<',';comma=true;
        std::cout<<"{\"name\":\""<<p.name<<"\",\"writes\":[";
        bool c=false;for(auto b:p.writes){if(c)std::cout<<',';c=true;std::cout<<b;}
        std::cout<<"],\"reads\":[";c=false;
        for(auto r:p.reads){if(c)std::cout<<',';c=true;std::cout<<'['<<r.tick<<','<<r.msr<<','<<r.value<<']';}
        std::cout<<"],\"idle_msr\":"<<p.idle<<'}';
    }
    std::cout<<"]}\n";
 } catch(const std::exception& e) { std::cerr<<e.what()<<'\n';return 2; }
}
