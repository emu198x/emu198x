// Research host: base-item wiring only. Controller source remains unchanged.
#pragma once
#include "FloppyDiskDrive.h"
#include <algorithm>
#include <cassert>
#include <cstring>
#include <memory>
using cstr=const char*;
using isa_id=int;
using Internal=bool;
class Machine {};
constexpr bool yes=true,no=false,on=true,off=false;
using std::min;
#define NELEM(x) (sizeof(x)/sizeof((x)[0]))
#define IERR() throw std::runtime_error("zxsp internal state error")
#define xlogline(...) ((void)0)
#define xxlog(...) ((void)0)
#define logline(...) ((void)0)
class Fdc {
protected:
    std::shared_ptr<FloppyDiskDrive> fdd[4],drive;
    bool motor_on=true,interrupt=false;
    Fdc(Machine*,isa_id,Internal,cstr,cstr) {}
    virtual ~Fdc()=default;
    virtual void powerOn(int32) { interrupt=false; }
    virtual void reset(Time,int32) { interrupt=false; }
    virtual void input(Time,int32,uint16,uint8&,uint8&)=0;
    virtual void output(Time,int32,uint16,uint8)=0;
    virtual void audioBufferEnd(Time) {}
    virtual void initForSnapshot(int32) {}
    virtual void raise_interrupt() { interrupt=true; }
    virtual void clear_interrupt() { interrupt=false; }
    virtual void send_byte_to_dma(uint8) { throw std::runtime_error("unexpected DMA"); }
    virtual uint8 read_byte_from_dma() { throw std::runtime_error("unexpected DMA"); }
};
