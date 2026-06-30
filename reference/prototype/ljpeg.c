/* Minimal DNG lossless-JPEG (SOF3) decoder.
 * Reads a raw JPEG strip (extracted from a DNG SubIFD) on stdin-arg file,
 * writes decoded CFA samples as little-endian uint16 to output file.
 * Handles: SOF3, single DHT (shared), N components interleaved (H=V=1),
 * predictor selector 1, optional restart markers. */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>

static uint8_t *buf; static long blen, bpos;
/* bit reader with FF-stuffing */
static uint32_t bitbuf; static int bitcnt;
static int marker_hit = 0;

static int nextbyte(void){
    if(bpos>=blen) return -1;
    return buf[bpos++];
}
static void fillbit(void){
    /* read one byte handling stuffing */
    if(bpos>=blen){ bitbuf=(bitbuf<<8); bitcnt+=8; return; }
    int c=buf[bpos++];
    if(c==0xFF){
        int c2 = (bpos<blen)? buf[bpos]:0;
        if(c2==0x00){ bpos++; }
        else if(c2>=0xD0 && c2<=0xD7){ /* restart marker: shouldn't be consumed here */ marker_hit=c2; c=0; bpos++; }
        else { marker_hit=c2; c=0; }
    }
    bitbuf=(bitbuf<<8)|c; bitcnt+=8;
}
static int getbit(void){
    if(bitcnt==0) fillbit();
    bitcnt--;
    return (bitbuf>>bitcnt)&1;
}
static int getbits(int n){
    int v=0; while(n--) v=(v<<1)|getbit(); return v;
}

/* Huffman table */
typedef struct { int mincode[17], maxcode[17], valptr[17]; uint8_t vals[256]; int nvals; } Huff;
static Huff H[4];

static void build_huff(Huff*h, uint8_t*counts, uint8_t*vals){
    int code=0,k=0; int huffsize[257]; int p=0;
    for(int l=1;l<=16;l++) for(int i=0;i<counts[l-1];i++) huffsize[p++]=l;
    huffsize[p]=0; int nv=p;
    h->nvals=nv; memcpy(h->vals,vals,nv);
    int huffcode[257]; p=0; code=0; int si=huffsize[0];
    while(huffsize[p]){
        while(huffsize[p]==si){ huffcode[p++]=code++; }
        code<<=1; si++;
    }
    p=0;
    for(int l=1;l<=16;l++){
        if(counts[l-1]){ h->valptr[l]=p; h->mincode[l]=huffcode[p]; p+=counts[l-1]; h->maxcode[l]=huffcode[p-1]; }
        else h->maxcode[l]=-1;
    }
}
static int huff_decode(Huff*h){
    int code=getbit(); int l=1;
    while(l<=16 && (h->maxcode[l]<0 || code>h->maxcode[l])){
        code=(code<<1)|getbit(); l++;
    }
    if(l>16) return 0;
    int idx=h->valptr[l]+(code-h->mincode[l]);
    return h->vals[idx];
}
static int extend(int v,int s){
    if(s==0) return 0;
    return (v < (1<<(s-1))) ? v-(1<<s)+1 : v;
}

int main(int argc,char**argv){
    if(argc<5){fprintf(stderr,"usage: ljpeg in.jpg out.raw Xfull Yfull\n");return 1;}
    FILE*f=fopen(argv[1],"rb"); fseek(f,0,SEEK_END); blen=ftell(f); fseek(f,0,SEEK_SET);
    buf=malloc(blen); fread(buf,1,blen,f); fclose(f);
    int Xfull=atoi(argv[3]), Yfull=atoi(argv[4]);

    int P=0,X=0,Y=0,Nf=0; int comp_td[4]={0,0,0,0};
    int Ri=0; /* restart interval */
    /* parse markers */
    bpos=0;
    if(!(buf[0]==0xFF&&buf[1]==0xD8)){fprintf(stderr,"no SOI\n");return 1;}
    bpos=2;
    int sos=0;
    while(bpos<blen && !sos){
        if(buf[bpos]!=0xFF){bpos++;continue;}
        int m=buf[bpos+1]; bpos+=2;
        if(m==0xD9) break;
        int L=(buf[bpos]<<8)|buf[bpos+1];
        uint8_t*seg=buf+bpos+2; int slen=L-2;
        if(m==0xC3){ /* SOF3 */
            P=seg[0]; Y=(seg[1]<<8)|seg[2]; X=(seg[3]<<8)|seg[4]; Nf=seg[5];
            for(int c=0;c<Nf;c++){ /* id,HV,Tq at 6+3c */ }
        } else if(m==0xC4){ /* DHT (may contain multiple tables) */
            int o=0;
            while(o<slen){
                int tcth=seg[o++]; int th=tcth&15; uint8_t counts[16]; int tot=0;
                for(int i=0;i<16;i++){counts[i]=seg[o+i];tot+=counts[i];} o+=16;
                build_huff(&H[th],counts,seg+o); o+=tot;
            }
        } else if(m==0xDD){ Ri=(seg[0]<<8)|seg[1]; }
        else if(m==0xDA){ /* SOS */
            int Ns=seg[0]; int o=1;
            for(int c=0;c<Ns;c++){ int cs=seg[o]; int td=seg[o+1]>>4; comp_td[c]=td; o+=2; }
            /* o now Ss,Se,AhAl */
            bpos = (seg+o+3)-buf; /* entropy data begins */
            sos=1; break;
        }
        bpos+=L;
    }
    if(!sos){fprintf(stderr,"no SOS\n");return 1;}
    if(Nf<1||Nf>4){fprintf(stderr,"bad Nf %d\n",Nf);return 1;}

    /* output full-res single-plane: width Xfull = X*Nf */
    uint16_t*out=calloc((size_t)Xfull*Yfull,2);
    int prev[4]; /* left reconstructed per comp */
    uint16_t*rowabove=calloc((size_t)X*Nf,2);
    uint16_t*rowcur=calloc((size_t)X*Nf,2);
    int half=1<<(P-1);
    int restart_cnt=0;
    bitcnt=0;bitbuf=0;

    for(int y=0;y<Y;y++){
        for(int c=0;c<Nf;c++) prev[c]=0;
        for(int x=0;x<X;x++){
            for(int c=0;c<Nf;c++){
                int s=huff_decode(&H[comp_td[c]]);
                int diff;
                if(s==16) diff=32768; else { int v=getbits(s); diff=extend(v,s); }
                int pred;
                if(x==0){
                    if(y==0) pred=half;
                    else pred=rowabove[c]; /* Rb */
                } else {
                    pred=prev[c]; /* Ra */
                }
                int val=(pred+diff)&0xFFFF;
                rowcur[x*Nf+c]=val; prev[c]=val;
                out[(size_t)y*Xfull + x*Nf + c]=val;
                /* restart handling */
            }
        }
        memcpy(rowabove,rowcur,(size_t)X*Nf*2);
        (void)restart_cnt;(void)nextbyte;
    }
    FILE*of=fopen(argv[2],"wb");
    fwrite(out,2,(size_t)Xfull*Yfull,of); fclose(of);
    fprintf(stderr,"decoded P=%d X=%d Y=%d Nf=%d -> %dx%d\n",P,X,Y,Nf,Xfull,Yfull);
    return 0;
}
