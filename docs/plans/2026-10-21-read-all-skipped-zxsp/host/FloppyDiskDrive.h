// Research host: decoded rotating bytes only, no controller/status semantics.
#pragma once
#include <cstdint>
#include <vector>
#include <cmath>
#include <stdexcept>
using uint8 = uint8_t;
using uint16 = uint16_t;
using uint32 = uint32_t;
using int32 = int32_t;
using uint = unsigned;
using Time = double;
class FloppyDiskDrive {
public:
    std::vector<uint8> bytes;
    bool is_ready=true, is_error=false, is_2sided=false, is_atindex=false;
    bool is_wprot=false, is_track0=true;
    uint track=0;
    Time now=0;
    uint reads=0, writes=0;
    void update(Time t) { now=t; is_atindex=bytePosition()<125; }
    uint bytesPerTrack() { return uint(bytes.size()); }
    Time timePerByte() { return 0.000032; }
    Time timePerTrack() { return bytes.size()*timePerByte(); }
    Time timeSinceIndex() { return std::fmod(now,timePerTrack()); }
    uint bytePosition() { return uint(std::floor(timeSinceIndex()/timePerByte())) % bytes.size(); }
    uint bytePosition(Time t) { update(t); return bytePosition(); }
    uint8 readByte(uint head,uint pos) {
        if(head || pos>=bytes.size()) throw std::runtime_error("invalid media address");
        ++reads; return bytes.at(pos);
    }
    void writeByte(uint head,uint pos,uint8 b) {
        if(head || pos>=bytes.size()) throw std::runtime_error("invalid media address");
        ++writes; bytes.at(pos)=b;
    }
    void step(Time t,int dir) { update(t); track=uint(int(track)+dir); is_track0=track==0; }
};
