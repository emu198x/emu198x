// Presentation only: the worker's native RGBA frame remains the capture source.
// The visible canvas stays 2-D so a failed/lost GPU context has a real fallback.
const vertexSource = `
attribute vec2 position;
varying vec2 uv;
void main() {
  gl_Position = vec4(position, 0.0, 1.0);
  uv = vec2((position.x + 1.0) * 0.5, (1.0 - position.y) * 0.5);
}`;
const fragmentSource = `
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
uniform sampler2D image;
uniform vec2 size;
uniform vec2 scale;
uniform bool sharp;
varying vec2 uv;
vec4 texel(vec2 p) {
  return texture2D(image, (clamp(p, vec2(0.0), size - 1.0) + 0.5) / size);
}
void main() {
  // Same sharp-bilinear reconstruction as emu198x-native-video/shader.wgsl.
  vec2 p = uv * size - 0.5;
  vec2 base = floor(p);
  vec2 t = fract(p);
  if (sharp) t = clamp((t - 0.5) * max(vec2(1.0), scale) + 0.5, 0.0, 1.0);
  gl_FragColor = mix(mix(texel(base), texel(base + vec2(1.0, 0.0)), t.x),
                     mix(texel(base + vec2(0.0, 1.0)), texel(base + 1.0), t.x), t.y);
}`;

function gpuPresenter(surface) {
  const gl = surface.getContext('webgl', {alpha:false, antialias:false, depth:false, stencil:false});
  if (!gl) return null;
  const shaders = [], resources = [];
  const release = () => {
    for (const shader of shaders) gl.deleteShader(shader);
    for (const [kind, resource] of resources) gl[`delete${kind}`](resource);
  };
  try {
    const compile = (kind, source) => {
      const shader = gl.createShader(kind);
      if (!shader) throw new Error('Shader allocation failed.');
      shaders.push(shader); gl.shaderSource(shader, source); gl.compileShader(shader);
      if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) throw new Error('Shader compilation failed.');
      return shader;
    };
    const program = gl.createProgram();
    if (!program) throw new Error('Program allocation failed.');
    resources.push(['Program', program]);
    gl.attachShader(program, compile(gl.VERTEX_SHADER, vertexSource));
    gl.attachShader(program, compile(gl.FRAGMENT_SHADER, fragmentSource));
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error('Shader link failed.');
    gl.useProgram(program);
    const buffer = gl.createBuffer(), texture = gl.createTexture();
    if (buffer) resources.push(['Buffer', buffer]);
    if (texture) resources.push(['Texture', texture]);
    if (!buffer || !texture) throw new Error('Frame allocation failed.');
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1,-1, 1,-1, -1,1, 1,1]), gl.STATIC_DRAW);
    const position = gl.getAttribLocation(program, 'position');
    gl.enableVertexAttribArray(position); gl.vertexAttribPointer(position, 2, gl.FLOAT, false, 0, 0);
    gl.bindTexture(gl.TEXTURE_2D, texture);
    for (const parameter of [gl.TEXTURE_MIN_FILTER, gl.TEXTURE_MAG_FILTER]) gl.texParameteri(gl.TEXTURE_2D, parameter, gl.NEAREST);
    for (const parameter of [gl.TEXTURE_WRAP_S, gl.TEXTURE_WRAP_T]) gl.texParameteri(gl.TEXTURE_2D, parameter, gl.CLAMP_TO_EDGE);
    const size = gl.getUniformLocation(program, 'size'), scale = gl.getUniformLocation(program, 'scale');
    const sharp = gl.getUniformLocation(program, 'sharp');
    gl.uniform1i(gl.getUniformLocation(program, 'image'), 0);
    let uploaded, width = 0, height = 0;
    return {
      draw(frame, viewport, isSharp) {
        if (gl.isContextLost()) return false;
        if (uploaded !== frame) {
          if (width !== frame.width || height !== frame.height) {
            gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, frame.width, frame.height, 0, gl.RGBA, gl.UNSIGNED_BYTE, frame.pixels);
            width = frame.width; height = frame.height;
          } else gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, width, height, gl.RGBA, gl.UNSIGNED_BYTE, frame.pixels);
          uploaded = frame;
        }
        gl.viewport(0, 0, surface.width, surface.height); gl.clearColor(0,0,0,1); gl.clear(gl.COLOR_BUFFER_BIT);
        gl.viewport(viewport.x, surface.height - viewport.y - viewport.height, viewport.width, viewport.height);
        gl.uniform2f(size, width, height);
        gl.uniform2f(scale, viewport.width / width, viewport.height / height);
        gl.uniform1i(sharp, isSharp ? 1 : 0);
        gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
        return gl.getError() === gl.NO_ERROR && !gl.isContextLost();
      },
      destroy() { release(); },
    };
  } catch {
    release(); return null;
  }
}

export function frameViewport(width, height, sourceWidth, sourceHeight, pixelAspectRatio) {
  const scale = Math.min(width / (sourceWidth * pixelAspectRatio), height / sourceHeight);
  const w = Math.max(1, Math.round(sourceWidth * pixelAspectRatio * scale));
  const h = Math.max(1, Math.round(sourceHeight * scale));
  return {x:Math.floor((width-w)/2), y:Math.floor((height-h)/2), width:w, height:h};
}

export function framePresenter(canvas) {
  const context = canvas.getContext('2d');
  const listeners = new AbortController();
  let frame, gpu, surface, source, sourceContext, sharp = true, disposed = false, backend = '2d';
  const size = () => frame ? [frame.width, frame.height] : [canvas.width, canvas.height];
  const aspect = () => frame?.pixelAspectRatio || 1;
  function render() {
    if (disposed || !frame) return;
    const rect = canvas.getBoundingClientRect();
    if (!rect.width || !rect.height) return;
    const dpr = window.devicePixelRatio || 1;
    const width = Math.max(1, Math.round(rect.width*dpr)), height = Math.max(1, Math.round(rect.height*dpr));
    if (canvas.width !== width) canvas.width = width;
    if (canvas.height !== height) canvas.height = height;
    const viewport = frameViewport(width, height, frame.width, frame.height, aspect());
    if (!surface) {
      surface = document.createElement('canvas');
      surface.addEventListener('webglcontextlost', event => {
        event.preventDefault(); gpu?.destroy(); gpu = null; render();
      }, {signal:listeners.signal});
      surface.addEventListener('webglcontextrestored', () => {
        if (disposed) return;
        gpu = gpuPresenter(surface); render();
      }, {signal:listeners.signal});
      gpu = gpuPresenter(surface);
    }
    if (surface.width !== width) surface.width = width;
    if (surface.height !== height) surface.height = height;
    context.imageSmoothingEnabled = false;
    context.clearRect(0,0,width,height);
    if (gpu?.draw(frame, viewport, sharp)) {
      backend = 'webgl';
      context.drawImage(surface,0,0); // Already filtered at device resolution.
    } else {
      backend = '2d';
      if (!source) { source = document.createElement('canvas'); sourceContext = source.getContext('2d'); }
      if (source.width !== frame.width) source.width = frame.width;
      if (source.height !== frame.height) source.height = frame.height;
      sourceContext.putImageData(new ImageData(new Uint8ClampedArray(frame.pixels.buffer, frame.pixels.byteOffset, frame.pixels.byteLength), frame.width, frame.height),0,0);
      context.fillStyle = '#000'; context.fillRect(0,0,width,height);
      context.imageSmoothingEnabled = !sharp;
      context.drawImage(source,viewport.x,viewport.y,viewport.width,viewport.height);
    }
  }
  // ResizeObserver also runs for fullscreen/container changes while paused.
  const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(render);
  observer?.observe(canvas);
  window.addEventListener('resize', render, {signal:listeners.signal});
  // Moving a paused window between monitors can change DPR without its CSS size.
  let resolution;
  const watchResolution = () => {
    resolution?.removeEventListener('change', resolutionChanged);
    resolution = window.matchMedia?.(`(resolution: ${window.devicePixelRatio || 1}dppx)`);
    resolution?.addEventListener('change', resolutionChanged);
  };
  const resolutionChanged = () => { watchResolution(); render(); };
  watchResolution();
  return {
    draw(value) {
      if (disposed || !value.pixels.length) return;
      if (!Number.isInteger(value.width) || value.width <= 0 || !Number.isInteger(value.height) || value.height <= 0 || value.pixels.length !== value.width*value.height*4) throw new Error('Invalid native frame.');
      const par = Number.isFinite(value.pixelAspectRatio) && value.pixelAspectRatio > 0 ? value.pixelAspectRatio : 1;
      frame = {pixels:value.pixels, width:value.width, height:value.height, pixelAspectRatio:par};
      canvas.style.aspectRatio = `${frame.width*par} / ${frame.height}`;
      render();
    },
    setSharp(value) { sharp = Boolean(value); render(); },
    size, aspect,
    get backend() { return backend; },
    destroy() {
      disposed = true; observer?.disconnect(); listeners.abort();
      resolution?.removeEventListener('change', resolutionChanged);
      gpu?.destroy(); surface?.getContext('webgl')?.getExtension('WEBGL_lose_context')?.loseContext();
      frame = gpu = surface = source = sourceContext = null;
    },
  };
}
