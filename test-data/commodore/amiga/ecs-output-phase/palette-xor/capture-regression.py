from pathlib import Path
import concurrent.futures, hashlib, importlib.util, json, subprocess
root=Path('/Users/stevehill/Projects/198x/Emu198x/emu198x')
base=Path('/private/tmp/emu198x-xor-phase/regression');base.mkdir(exist_ok=True)
dirs=[Path(p) for p in ['/private/tmp/emu198x-display-phase-sweep/ddf30','/private/tmp/emu198x-display-phase-sweep/ddf38','/private/tmp/emu198x-midline-display/guests','/private/tmp/emu198x-midline-display/varying','/private/tmp/emu198x-midline-display/reset','/private/tmp/emu198x-wide-sprite-validation/guests','/private/tmp/emu198x-wide-sprite-validation/playfields','/private/tmp/emu198x-midline-scroll/all-widths','/private/tmp/emu198x-midline-scroll/scroll-sweep','/private/tmp/emu198x-midline-scroll/scroll-sweep-ddf30']]
dirs = [d for d in dirs if d.parent.name == 'emu198x-midline-display']
cases=[(directory,record) for directory in dirs for record in json.loads((directory/'diagnostics.json').read_text())['cases']]
assert len(cases)==24 and len({str(d/r['case']) for d,r in cases})==24
provenance=json.loads(Path('/private/tmp/emu198x-area-dma-probe/input-provenance.json').read_text())
assert provenance['cases']==128 and len(provenance['records'])==128
for record in provenance['records']:
    assert hashlib.sha256((Path(record['path'])/'probe.adf').read_bytes()).hexdigest()==record['adf_sha256']

spec=importlib.util.spec_from_file_location('common_compare',root/'test-data/commodore/amiga/wide-sprite-dma/tools/compare.py');assert spec and spec.loader
compare=importlib.util.module_from_spec(spec);spec.loader.exec_module(compare)
def native(entry):
    directory,record=entry;path=directory/record['case']
    assert (path/'probe.adf').is_file()
    if 'adf_sha256' in record:assert hashlib.sha256((path/'probe.adf').read_bytes()).hexdigest()==record['adf_sha256']
    script=path/'xor54.json';script.write_text(json.dumps([{'action':'run_frames','frames':360},{'action':'save_screenshot','path':str(path/'xor54.png')},{'action':'memory_read','addr':0x2ff00,'len':128}]))
    with (path/'xor54.log').open('w') as log:
        subprocess.run([str(root/'target/release/emu198x-amiga'),'--model','a1200','--kickstart',str(root/'../roms/kick31_40_068_a1200.rom'),'--disk',str(path/'probe.adf'),'--script',str(script)],stdout=log,stderr=subprocess.STDOUT,check=True)
    observations=json.loads((path/'xor54.log').read_text())['observations']
    record=next(item for item in observations if item['kind']=='memory_read')
    ready=bytes(record['bytes']); assert ready[:4]==b'SPHX' and int.from_bytes(ready[8:12],'big')>=9
    assert ready[64:].split(b'\0',1)[0].decode()==json.loads((path/'inputs.json').read_text())['identity']['serial']
    result=compare.compare(path,'xor54.png');result['path']=str(path)
    result['historical_mapping_fields']=result['fields']
    from PIL import Image, ImageChops
    native_image=Image.open(path/'xor54.png').convert('RGB')
    assert native_image.size==(1536,576)
    actual=native_image.crop((16,2,1524,576))
    fields=[]
    for raw in sorted((path/'reference/capture').glob('*.bgra')):
        meta=json.loads(raw.with_suffix('.json').read_text())['framebuffer']
        assert (meta['inbuffer_xoffset'],meta['inbuffer_yoffset'],meta['width'],meta['height'],meta['host_resolution'])==(368,52,1512,576,2)
        ref=Image.frombytes('RGBA',(1512,576),raw.read_bytes(),'raw','BGRA').convert('RGB').crop((4,0,1512,574))
        diff=ImageChops.difference(actual,ref)
        fields.append(dict(raw_sha256=hashlib.sha256(raw.read_bytes()).hexdigest(),mismatched_rgb_pixels=sum(p!=(0,0,0) for p in diff.get_flattened_data()),compared_rgb_pixels=1508*574,mismatch_bounds=diff.getbbox()))
    assert len(fields)==3
    result['fields']=fields
    result['origin_evidence']='Shared registered producer output padding, independently counter-traced on ECS/Lisa controls across lores/hires/superhires; retained corpus fields do not contain per-case counter traces.'
    result['common_raster']=dict(native_origin=[16,2],reference_origin=[4,0],width=1508,height=574)
    print(path, sum(field['mismatched_rgb_pixels'] for field in result['fields']),flush=True)
    return result
paths=[root/'target/release/emu198x-amiga',root/'../roms/kick31_40_068_a1200.rom']+[d/r['case']/'probe.adf' for d,r in cases]
def hashes(): return {str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
paths += [p for d,r in cases for p in (d/r['case']/'reference/capture').iterdir() if p.suffix in ('.bgra','.json')]
before=hashes()
(base/'producers.json').write_text(json.dumps(before,indent=2)+'\n')
with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool: results=list(pool.map(native,cases))
assert hashes()==before, 'producer/input mutated during capture'
assert len(results)==24
failed=sum(field['mismatched_rgb_pixels']!=0 for case in results for field in case['fields'])
(base/'corrected-mapping.json').write_text(json.dumps(dict(compared_fields=72,failed_fields=failed,results=results),indent=2)+'\n')
if failed: raise SystemExit(1)
