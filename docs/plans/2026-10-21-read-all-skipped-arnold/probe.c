/* SPDX-License-Identifier: GPL-2.0-or-later
 * Public-register component probe. Disk adapter has no command/SK/status policy.
 */
#include "fdc.h"
#include "fdi.h"
#include "fdd.h"
#include "diskimage/diskimg.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static unsigned long tick, deadline, poll_us;
static unsigned char media[2048];
static size_t media_size;
static int count;
static unsigned ids, sectors, media_bytes;
static int phase_first=1;

static void die(const char *s) { fprintf(stderr,"%s\n",s);exit(2); }
unsigned long CPC_GetNopCount(void) { return tick; }
static void location(int drive,int track,int side,int index) {
    if(drive!=0 || track!=0 || side!=0 || index<0 || index>=count)die("invalid media address");
}
int DiskImage_GetSectorsPerTrack(int drive,int track,int side) {
    if(drive!=0 || track!=0 || side!=0)return 0;
    return count;
}
void DiskImage_GetID(int drive,int track,int side,int index,CHRN *id) {
    location(drive,track,side,index);
    unsigned char *p=media+280+index*8;
    id->C=p[0];id->H=p[1];id->R=p[2];id->N=p[3];id->ST1=p[4];id->ST2=p[5];++ids;
}
void DiskImage_GetSector(int drive,int track,int side,int index,char *buffer) {
    location(drive,track,side,index);
    memcpy(buffer,media+512+index*512,512);++sectors;media_bytes+=512;
}
void DiskImage_PutSector(int d,int t,int s,int i,char *b,int m) { (void)d;(void)t;(void)s;(void)i;(void)b;(void)m;die("unexpected disk write"); }
void DiskImage_AddSector(int d,int t,int s,CHRN *id,int n,int f,int g) { (void)d;(void)t;(void)s;(void)id;(void)n;(void)f;(void)g;die("unexpected format"); }
void DiskImage_EmptyTrack(int d,int t,int s) { (void)d;(void)t;(void)s;die("unexpected erase"); }

static unsigned status(void) {
    tick+=poll_us;
    if(tick>deadline || tick>8000000)die("emulated command/time limit");
    FDC_Cycle();
    return FDC_ReadMainStatusRegister();
}
static void command(const char *name,const unsigned char *bytes,unsigned n) {
    deadline=tick+2000000;
    if(!phase_first)printf(",");phase_first=0;
    printf("{\"name\":\"%s\",\"writes\":[",name);
    for(unsigned i=0;i<n;++i) {
        unsigned s;
        do{s=status();}while((s&0xC0)!=0x80);
        FDC_WriteDataRegister(bytes[i]);
        if(i)printf(",");printf("%u",bytes[i]);
    }
    printf("],\"reads\":[");
    unsigned reads=0;
    for(;;) {
        unsigned s=status();
        if((s&0xC0)==0xC0) {
            unsigned value=FDC_ReadDataRegister();
            if(reads++)printf(",");
            printf("[%lu,%u,%u]",tick,s,value);
            if(reads>65536)die("read count limit");
        } else if(s==0x80) {
            printf("],\"idle_msr\":%u}",s);return;
        }
    }
}
int main(int argc,char **argv) {
    if(argc!=6)die("usage: probe input.edsk live.edsk opcode EOT poll_us");
    poll_us=strtoul(argv[5],NULL,0);if(poll_us!=1 && poll_us!=32)die("invalid poll interval");
    FILE *f=fopen(argv[1],"rb");if(!f)die("missing disk prerequisite");
    media_size=fread(media,1,sizeof(media),f);if(ferror(f)||!feof(f))die("invalid disk length");fclose(f);
    if(media_size<1024 || memcmp(media,"EXTENDED CPC DSK File\r\nDisk-Info\r\n",34)!=0 || media[48]!=1 || media[49]!=1 || memcmp(media+256,"Track-Info\r\n",12)!=0)die("unsupported authored disk");
    count=media[277];if(count<1||count>2||media_size!=(size_t)(512+512*count))die("invalid sector count/length");
    for(int i=0;i<count;++i) {
        unsigned char *p=media+280+i*8;
        if(p[0]||p[1]||p[2]!=i+1||p[3]!=2||p[6]||p[7]!=2)die("invalid sector layout");
    }
    FDD_InitialiseAll();FDD_SetTracks(0,43);FDD_SetDoubleSided(0,FALSE);FDD_Enable(0,TRUE);
    /* Put the mechanics at physical track0 independently of controller PCN. */
    FDD_GetDrive(0)->CurrentTrack=0;FDD_RefreshTrack0(0);
    FDD_InsertDisk(0,TRUE);FDI_SetMotorState(1);FDI_SetPhysicalDrive(0);FDI_SetPhysicalSide(0);
    FDC_Power();FDC_Reset();FDC_SetTerminalCount(0);
    printf("{\"tick_seconds\":0.000001,\"poll_us\":%lu,\"phases\":[",poll_us);
    const unsigned char specify[]={3,0xDF,3};command("specify",specify,3);
    unsigned char read[]={0,0,0,0,1,2,0,0x2A,0xFF};read[0]=(unsigned char)strtoul(argv[3],NULL,0);read[6]=(unsigned char)strtoul(argv[4],NULL,0);
    command("read",read,9);
    const unsigned char sds[]={4,0};command("recovery_sds",sds,2);
    printf("],\"total_ticks\":%lu,\"disk_ids\":%u,\"disk_reads\":%u,\"disk_bytes\":%u,\"disk_writes\":0}\n",tick,ids,sectors,media_bytes);
    f=fopen(argv[2],"wb");if(!f||fwrite(media,1,media_size,f)!=media_size||fclose(f)!=0)die("live disk output failed");
    return 0;
}
