/* ThePenMarket.com hero: a raymarched Parker-style laminated celluloid barrel (his Vacumatics),
   translucent amber and pearl laminations lit from behind, the ink level showing through, the
   two-tone nib catching one highlight. SDF raymarch in a single fragment shader; procedural layered
   material; pointer tilts the pen and the ink sloshes; the barrel rolls slowly. three r160 UMD.
   Reduced motion: one still frame. Off-screen: paused. No WebGL: the CSS band behind stays. */
(function () {
  "use strict";
  if (!window.THREE) return;
  var canvas = document.getElementById("scene"), hero = document.getElementById("hero");
  if (!canvas || !hero) return;
  var reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  var renderer;
  try { renderer = new THREE.WebGLRenderer({ canvas: canvas, antialias: false, alpha: true, powerPreference: "high-performance" }); } catch (e) { return; }
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 1.75));
  renderer.setClearColor(0x000000, 0);
  renderer.autoClear = true;

  var small = window.innerWidth < 720;
  var STEPS = small ? 44 : 72;
  var scene = new THREE.Scene();
  var camera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 1);
  var uniforms = {
    uTime: { value: 0 }, uRes: { value: new THREE.Vector2(1, 1) }, uTilt: { value: new THREE.Vector2(0, 0) },
    uLayout: { value: new THREE.Vector4(1.15, 0.05, 1.0, 1.0) } // x offset, y offset, scale, aspect flag
  };
  var frag = [
    "precision highp float;",
    "uniform float uTime; uniform vec2 uRes; uniform vec2 uTilt; uniform vec4 uLayout;",
    "varying vec2 vUv;",
    "#define STEPS " + STEPS,
    "float hash(vec3 p){ p = fract(p*0.3183099+.1); p *= 17.0; return fract(p.x*p.y*p.z*(p.x+p.y+p.z)); }",
    "float noise(vec3 x){ vec3 i = floor(x); vec3 f = fract(x); f = f*f*(3.0-2.0*f);",
    "  return mix(mix(mix(hash(i), hash(i+vec3(1,0,0)), f.x), mix(hash(i+vec3(0,1,0)), hash(i+vec3(1,1,0)), f.x), f.y),",
    "             mix(mix(hash(i+vec3(0,0,1)), hash(i+vec3(1,0,1)), f.x), mix(hash(i+vec3(0,1,1)), hash(i+vec3(1,1,1)), f.x), f.y), f.z); }",
    "float fbm(vec3 p){ float v = 0.0; float a = 0.5; for (int i = 0; i < 4; i++) { v += a*noise(p); p = p*2.02 + vec3(11.0, 7.0, 3.0); a *= 0.5; } return v; }",
    "mat3 rotX(float a){ float c = cos(a), s = sin(a); return mat3(1,0,0, 0,c,-s, 0,s,c); }",
    "mat3 rotY(float a){ float c = cos(a), s = sin(a); return mat3(c,0,s, 0,1,0, -s,0,c); }",
    "mat3 rotZ(float a){ float c = cos(a), s = sin(a); return mat3(c,-s,0, s,c,0, 0,0,1); }",
    "float sdCylX(vec3 p, float x0, float x1, float r){ float cx = 0.5*(x0+x1); float h = 0.5*(x1-x0); vec2 d = abs(vec2(length(p.yz), p.x-cx)) - vec2(r, h); return min(max(d.x,d.y),0.0) + length(max(d,0.0)); }",
    "float sdRoundCylX(vec3 p, float x0, float x1, float r, float rr){ return sdCylX(p, x0+rr, x1-rr, r-rr) - rr; }",
    "float sdConeX(vec3 p, float x0, float x1, float r0, float r1){ float t = clamp((p.x-x0)/(x1-x0), 0.0, 1.0); float r = mix(r0, r1, t); float d = length(p.yz) - r; float dx = max(x0 - p.x, p.x - x1); return max(d, dx); }",
    "float sdBox(vec3 p, vec3 b){ vec3 q = abs(p) - b; return length(max(q,0.0)) + min(max(q.x,max(q.y,q.z)),0.0); }",
    // scene: barrel + cap + band + section + nib + clip, along x
    "float map(vec3 p, out float mat){",
    "  float barrel = sdRoundCylX(p, -0.55, 1.05, 0.19, 0.06);",
    "  float cap = sdRoundCylX(p, -1.65, -0.42, 0.215, 0.07);",
    "  float capBand = sdCylX(p, -0.52, -0.44, 0.222);",
    "  float capTop = sdCylX(p, -1.66, -1.58, 0.16);",
    "  float section = sdCylX(p, 1.02, 1.20, 0.155);",
    "  vec3 q = p; q.z *= 2.4; float nib = sdConeX(q, 1.16, 1.95, 0.15, 0.018); nib = max(nib, -p.y - 0.03);",
    "  vec3 c = p - vec3(-1.15, 0.24, 0.0); float clip = sdBox(c, vec3(0.42, 0.018, 0.03)) - 0.01; clip = min(clip, sdBox(p - vec3(-1.56, 0.19, 0.0), vec3(0.03, 0.07, 0.03)) - 0.01);",
    "  float d = barrel; mat = 1.0;",
    "  if (cap < d) { d = cap; mat = 2.0; }",
    "  if (capBand < d) { d = capBand; mat = 3.0; }",
    "  if (capTop < d) { d = capTop; mat = 3.0; }",
    "  if (section < d) { d = section; mat = 4.0; }",
    "  if (nib < d) { d = nib; mat = 3.0; }",
    "  if (clip < d) { d = clip; mat = 3.0; }",
    "  return d; }",
    "vec3 normalAt(vec3 p){ float m; vec2 e = vec2(0.0015, 0.0); return normalize(vec3(map(p+e.xyy,m)-map(p-e.xyy,m), map(p+e.yxy,m)-map(p-e.yxy,m), map(p+e.yyx,m)-map(p-e.yyx,m))); }",
    // laminated celluloid: stacked translucent rings along the axis, pearl swirl, rolled by uTime
    "vec3 celluloid(vec3 p, vec3 n, vec3 rd, float roll, float ink, out float trans){",
    "  vec3 lp = rotX(roll) * p;",
    "  float ring = sin(lp.x * 46.0 + fbm(lp * 3.5) * 2.2 + 0.6 * sin(atan(lp.z, lp.y) * 3.0));",
    "  float amber = smoothstep(-0.25, 0.25, ring);",
    "  float pearl = fbm(vec3(lp.x * 6.0, atan(lp.z, lp.y) * 1.2, roll * 0.3));",
    "  vec3 amberCol = mix(vec3(0.62, 0.40, 0.14), vec3(0.86, 0.62, 0.24), pearl);",
    "  vec3 pearlCol = mix(vec3(0.80, 0.75, 0.62), vec3(0.96, 0.92, 0.80), pearl);",
    "  vec3 col = mix(pearlCol, amberCol, amber);",
    "  trans = amber * 0.85 + 0.05;",
    "  float below = smoothstep(0.02, -0.02, p.y - ink);",
    "  col = mix(col, mix(col, vec3(0.10, 0.14, 0.26), 0.7), below * trans * step(0.0, 1.05 - abs(p.x + 0.0)) );",
    "  return col; }",
    "void main(){",
    "  vec2 uv = (vUv * 2.0 - 1.0); uv.x *= uRes.x / uRes.y;",
    "  uv = (uv - uLayout.xy) / uLayout.z;",
    "  vec3 ro = vec3(0.0, 0.0, 4.2); vec3 rd = normalize(vec3(uv * 0.55, -1.0));",
    "  mat3 R = rotZ(-0.20 + uTilt.y * 0.10) * rotY(0.42 + uTilt.x * 0.22) * rotX(0.30);",
    "  vec3 ro2 = ro * R; vec3 rd2 = normalize(rd * R);",
    "  float t = 0.0; float mat = 0.0; float minD = 10.0; bool hit = false; vec3 p = ro2;",
    "  for (int i = 0; i < STEPS; i++) { p = ro2 + rd2 * t; float d = map(p, mat); minD = min(minD, d); if (d < 0.0015) { hit = true; break; } t += d * 0.9; if (t > 9.0) break; }",
    "  float roll = uTime * 0.18;",
    "  float ink = -0.03 + uTilt.x * 0.14 + 0.02 * sin(uTime * 0.7);",
    "  vec3 key = normalize(vec3(0.45, 0.85, 0.65)); vec3 back = normalize(vec3(-0.6, 0.35, -0.9)); vec3 fill = normalize(vec3(-0.5, -0.3, 0.7));",
    "  vec3 col = vec3(0.0); float alpha = 0.0;",
    "  vec3 halo = vec3(0.86, 0.62, 0.24) * exp(-minD * 7.0) * 0.55;",
    "  if (hit) {",
    "    vec3 n = normalAt(p); vec3 v = -rd2;",
    "    float dif = max(dot(n, key), 0.0); float rim = pow(1.0 - max(dot(n, v), 0.0), 3.0); float spec = pow(max(dot(reflect(-key, n), v), 0.0), 60.0);",
    "    float backlit = max(dot(n, back), 0.0);",
    "    if (mat < 2.5) {",
    "      float trans; vec3 base = celluloid(p, n, rd2, roll + (mat > 1.5 ? 0.9 : 0.0), ink, trans);",
    "      col = base * (0.30 + 0.75 * dif) + base * backlit * trans * 1.1 + vec3(1.0, 0.92, 0.7) * spec * 0.55 + vec3(0.95, 0.75, 0.35) * rim * (0.25 + 0.45 * trans);",
    "      col += vec3(0.86, 0.62, 0.24) * trans * 0.18;",
    "    } else if (mat < 3.5) {",
    "      vec3 gold = vec3(0.86, 0.66, 0.30); vec3 refl = reflect(rd2, n); float env = 0.35 + 0.65 * smoothstep(-0.4, 0.8, refl.y);",
    "      col = gold * (0.25 + 0.6 * dif) * env + vec3(1.0, 0.95, 0.8) * pow(max(dot(reflect(-key, n), v), 0.0), 120.0) * 1.2 + gold * rim * 0.5 + vec3(0.9, 0.8, 0.6) * backlit * 0.15;",
    "      float slit = smoothstep(0.008, 0.0, abs(p.z)) * step(1.35, p.x) * step(p.x, 1.78); col = mix(col, vec3(0.2, 0.17, 0.1), slit * 0.8);",
    "    } else {",
    "      col = vec3(0.09, 0.08, 0.07) * (0.6 + 0.6 * dif) + vec3(0.6, 0.55, 0.45) * spec * 0.5 + vec3(0.4) * rim * 0.2;",
    "    }",
    "    float fog = smoothstep(2.0, 6.5, t); col = mix(col, vec3(0.07, 0.08, 0.11), fog * 0.5);",
    "    alpha = 1.0;",
    "  } else { col = halo; alpha = clamp(length(halo) * 1.6, 0.0, 0.7); }",
    "  col = pow(col, vec3(0.92));",
    "  gl_FragColor = vec4(col, alpha);",
    "}"
  ].join("\n");
  var vert = "varying vec2 vUv; void main(){ vUv = uv; gl_Position = vec4(position.xy, 0.0, 1.0); }";
  var mat = new THREE.ShaderMaterial({ uniforms: uniforms, vertexShader: vert, fragmentShader: frag, transparent: true, depthTest: false, depthWrite: false });
  scene.add(new THREE.Mesh(new THREE.PlaneGeometry(2, 2), mat));

  function layout() {
    var w = hero.clientWidth, h = canvas.parentElement ? canvas.parentElement.clientHeight : hero.clientHeight;
    if (!w || !h) return;
    renderer.setSize(w, h, false);
    uniforms.uRes.value.set(w, h);
    var aspect = w / h;
    if (w < 720) { uniforms.uLayout.value.set(0.0, 0.05, 2.3, 1.0); }
    else if (aspect > 1.6) { uniforms.uLayout.value.set(aspect * 0.34, 0.42, 1.55, 1.0); }
    else if (aspect > 1.0) { uniforms.uLayout.value.set(aspect * 0.26, 0.45, 1.25, 1.0); }
    else { uniforms.uLayout.value.set(0.0, 0.05, 0.85, 1.0); }
  }
  layout();

  var target = new THREE.Vector2(0, 0);
  function onPointer(e) {
    var r = hero.getBoundingClientRect();
    var x = (e.clientX - r.left) / r.width * 2 - 1, y = (e.clientY - r.top) / r.height * 2 - 1;
    target.set(Math.max(-1, Math.min(1, x)), Math.max(-1, Math.min(1, y)));
  }
  var start = performance.now();
  function render(sec) { uniforms.uTime.value = sec; renderer.render(scene, camera); }
  if (reduce) { render(2.5); return; }
  hero.addEventListener("pointermove", onPointer, { passive: true });
  hero.addEventListener("pointerleave", function () { target.set(0, 0); }, { passive: true });

  var running = true, raf = 0;
  function loop(now) {
    if (!running) return;
    uniforms.uTilt.value.lerp(target, 0.05);
    render((now - start) / 1000);
    raf = requestAnimationFrame(loop);
  }
  raf = requestAnimationFrame(loop);
  if ("IntersectionObserver" in window) {
    new IntersectionObserver(function (entries) {
      var vis = entries[0].isIntersecting;
      if (vis && !running) { running = true; raf = requestAnimationFrame(loop); }
      if (!vis && running) { running = false; cancelAnimationFrame(raf); }
    }, { threshold: 0.02 }).observe(hero);
  }
  window.addEventListener("resize", function () { small = window.innerWidth < 720; layout(); }, { passive: true });
})();
