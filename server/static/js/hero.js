/* The 3D pen. Built from the newest pen in the catalog: the canvas carries that pen's photo, trim,
   pattern, kind (fountain / ballpoint / pencil), mechanism, nib and era. Colours are sampled from
   Nathaniel's photograph; the model is a posted fountain pen (cap on the end, nib down) turned in
   three.js with a procedural room reflection. Static under reduced motion, paused off-screen, and
   the photograph stays if WebGL is unavailable. */
(function () {
  "use strict";
  var canvas = document.getElementById("scene");
  if (!canvas || !window.THREE) return;
  var T = window.THREE, stage = canvas.parentElement, hotspotsEl = document.getElementById("hotspots");
  var d = canvas.dataset;
  var reduced = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  var renderer;
  try {
    renderer = new T.WebGLRenderer({ canvas: canvas, antialias: true, alpha: true, powerPreference: "high-performance" });
  } catch (e) { return; }
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
  if ("outputColorSpace" in renderer && T.SRGBColorSpace) renderer.outputColorSpace = T.SRGBColorSpace; else if (T.sRGBEncoding) renderer.outputEncoding = T.sRGBEncoding;
  renderer.toneMapping = T.ACESFilmicToneMapping; renderer.toneMappingExposure = 1.05;

  var scene = new T.Scene();
  var camera = new T.PerspectiveCamera(30, 1, 0.1, 100);
  camera.position.set(0, 0.6, 15);
  camera.lookAt(0, 0, 0);

  // A small bright room, baked to an environment map so the metal trim and clearcoat have something to reflect.
  function roomEnvironment() {
    var room = new T.Scene();
    var geo = new T.SphereGeometry(20, 32, 16);
    var mat = new T.MeshBasicMaterial({ color: 0xdcdad4, side: T.BackSide });
    room.add(new T.Mesh(geo, mat));
    var floor = new T.Mesh(new T.PlaneGeometry(60, 60), new T.MeshBasicMaterial({ color: 0x5a5854 })); floor.rotation.x = -Math.PI / 2; floor.position.y = -8; room.add(floor);
    var lightMat = new T.MeshBasicMaterial({ color: 0xffffff });
    var panels = [[-6, 6, -4, 5, 9], [7, 4, 2, 4, 6], [0, -7, 6, 8, 3], [2, 8, 8, 6, 4]];
    panels.forEach(function (p) {
      var m = new T.Mesh(new T.PlaneGeometry(p[3], p[4]), lightMat);
      m.position.set(p[0], p[1], p[2]); m.lookAt(0, 0, 0); room.add(m);
    });
    var pm = new T.PMREMGenerator(renderer); pm.compileEquirectangularShader();
    var env = pm.fromScene(room, 0.04).texture; pm.dispose();
    return env;
  }
  scene.environment = roomEnvironment();
  scene.add(new T.HemisphereLight(0xffffff, 0x6f6c66, 0.75));
  var key = new T.DirectionalLight(0xffffff, 1.6); key.position.set(5, 8, 7); scene.add(key);
  var fill = new T.DirectionalLight(0xffffff, 0.45); fill.position.set(-6, 1, 5); scene.add(fill);
  var rim = new T.DirectionalLight(0xffffff, 0.9); rim.position.set(-4, 4, -6); scene.add(rim);

  // ---- colours from the photograph ----------------------------------------------------------
  function samplePhoto(url, done) {
    if (!url) return done(null);
    var img = new Image();
    img.onload = function () {
      try {
        var cv = document.createElement("canvas"), w = cv.width = 96, h = cv.height = 64, g = cv.getContext("2d");
        g.drawImage(img, 0, 0, w, h);
        var px = g.getImageData(0, 0, w, h).data, bins = {}, i, r, gg, b, max, min, l, s, k;
        for (i = 0; i < px.length; i += 4) {
          r = px[i]; gg = px[i + 1]; b = px[i + 2];
          max = Math.max(r, gg, b); min = Math.min(r, gg, b); l = (max + min) / 510; s = max === min ? 0 : (max - min) / (255 - Math.abs(max + min - 255));
          if (l > 0.88) continue; // near-white
          if (l > 0.5 && s < 0.35 && r >= gg && gg >= b && (r - b) < 70) continue; // the beige backdrop his photos use
          k = (r >> 4) + "," + (gg >> 4) + "," + (b >> 4); bins[k] = (bins[k] || 0) + 1;
        }
        var keys = Object.keys(bins).sort(function (a, b2) { return bins[b2] - bins[a]; });
        if (!keys.length) return done(null);
        var top = keys.slice(0, 10).map(function (kk) { var p = kk.split(",").map(Number); return new T.Color((p[0] * 16 + 8) / 255, (p[1] * 16 + 8) / 255, (p[2] * 16 + 8) / 255); });
        // the commonest tone leads; the darkest of the common tones is kept so stripes and marbling have contrast
        var darkest = top.slice().sort(function (p, q) { return p.getHSL({}).l - q.getHSL({}).l; })[0];
        var out = top.slice(0, 3); if (out.indexOf(darkest) < 0) out.push(darkest);
        done(out);
      } catch (e) { done(null); }
    };
    img.onerror = function () { done(null); };
    img.src = url;
  }

  function barrelTexture(colors, pattern) {
    var cv = document.createElement("canvas"), w = cv.width = 1024, h = cv.height = 256, g = cv.getContext("2d");
    var sorted = colors.slice().sort(function (p, q) { return q.getHSL({}).l - p.getHSL({}).l; });
    var a = colors[0], b = sorted[0] === a ? (sorted[1] || a.clone().offsetHSL(0, 0, 0.18)) : sorted[0], c = sorted[sorted.length - 1] === a ? a.clone().offsetHSL(0, 0, -0.22) : sorted[sorted.length - 1];
    var la = a.getHSL({}).l, lc = c.getHSL({}).l;
    if (la - lc < 0.45) { c = c.clone(); var hsl = {}; c.getHSL(hsl); c.setHSL(hsl.h, Math.min(hsl.s, 0.5), Math.max(0.06, la - 0.5)); }
    var hex = function (col) { return "#" + col.getHexString(); };
    g.fillStyle = hex(a); g.fillRect(0, 0, w, h);
    var i, x, y;
    if (pattern === "stripes") {
      // lengthwise laminations: bands along u (around the barrel is v here), alternating three tones
      for (i = 0; i < 72; i++) {
        var bw = 5 + (i % 3) * 5; x = (i * 14.2) % w;
        g.fillStyle = i % 3 === 0 ? hex(b) : (i % 3 === 1 ? hex(c) : hex(a)); g.globalAlpha = 1; g.fillRect(x, 0, bw, h);
        g.fillStyle = hex(c); g.globalAlpha = 0.55; g.fillRect(x + bw, 0, 2, h);
      }
      g.globalAlpha = 1;
    } else if (pattern === "marble") {
      for (i = 0; i < 260; i++) {
        x = Math.random() * w; y = Math.random() * h; var rx = 20 + Math.random() * 120, ry = 6 + Math.random() * 30;
        g.beginPath(); g.ellipse(x, y, rx, ry, (Math.random() - 0.5) * 0.7, 0, Math.PI * 2);
        g.fillStyle = Math.random() < 0.5 ? hex(b) : hex(c); g.globalAlpha = 0.25 + Math.random() * 0.35; g.fill();
      }
      g.globalAlpha = 1;
    } else {
      for (i = 0; i < 1800; i++) { g.fillStyle = Math.random() < 0.5 ? hex(b) : hex(c); g.globalAlpha = 0.05; g.fillRect(Math.random() * w, Math.random() * h, 2, 2); }
      g.globalAlpha = 1;
    }
    var tex = new T.CanvasTexture(cv);
    tex.wrapS = tex.wrapT = T.RepeatWrapping;
    if ("colorSpace" in tex && T.SRGBColorSpace) tex.colorSpace = T.SRGBColorSpace; else if (T.sRGBEncoding) tex.encoding = T.sRGBEncoding;
    return tex;
  }

  // ---- the pen -----------------------------------------------------------------------------
  var pen = new T.Group(); scene.add(pen);
  var anchors = {};
  function lathe(points, mat, segs) { return new T.Mesh(new T.LatheGeometry(points.map(function (p) { return new T.Vector2(p[0], p[1]); }), segs || 96), mat); }

  function build(colors) {
    var trim = d.trim === "silver" ? 0xd7dade : 0xd4b05a;
    var metal = new T.MeshStandardMaterial({ color: trim, metalness: 1, roughness: 0.2, envMapIntensity: 1.6 });
    var black = new T.MeshPhysicalMaterial({ color: 0x151517, roughness: 0.3, clearcoat: 1, clearcoatRoughness: 0.08 });
    var body = new T.MeshPhysicalMaterial({ map: barrelTexture(colors, d.pattern), roughness: 0.22, clearcoat: 1, clearcoatRoughness: 0.05, reflectivity: 0.7, envMapIntensity: 1.3 });
    var kind = d.kind || "fountain";
    var R = 0.44; // barrel radius

    // barrel: from the section (y=0) up to the posted cap (y=3.6), slightly tapered
    var barrel = lathe([[R * 0.86, 0], [R, 0.3], [R, 3.0], [R * 0.98, 3.55], [R * 0.9, 3.65]], body); pen.add(barrel);
    // posted cap on the end of the barrel: y 3.6 .. 7.1, rounded top
    var capPts = [[R * 0.92, 3.6], [R * 1.06, 3.75], [R * 1.06, 6.55], [R * 1.0, 6.85], [R * 0.7, 7.05], [R * 0.3, 7.15], [0, 7.18]];
    var cap = lathe(capPts, body); pen.add(cap);
    // cap band and lip ring
    var band = new T.Mesh(new T.CylinderGeometry(R * 1.075, R * 1.075, 0.22, 96, 1, true), metal); band.position.y = 3.95; pen.add(band);
    var band2 = new T.Mesh(new T.CylinderGeometry(R * 1.075, R * 1.075, 0.06, 96, 1, true), metal); band2.position.y = 4.3; pen.add(band2);
    var jewel = new T.Mesh(new T.SphereGeometry(R * 0.32, 32, 16), black); jewel.position.y = 7.12; pen.add(jewel);
    // clip: a curved bar along the cap
    var clipCurve = new T.CatmullRomCurve3([new T.Vector3(0, 6.95, R * 0.9), new T.Vector3(0, 6.7, R * 1.28), new T.Vector3(0, 5.2, R * 1.24), new T.Vector3(0, 4.55, R * 1.2), new T.Vector3(0, 4.4, R * 1.12)]);
    var clip = new T.Mesh(new T.TubeGeometry(clipCurve, 40, 0.075, 12, false), metal); pen.add(clip);
    var clipBall = new T.Mesh(new T.SphereGeometry(0.11, 16, 12), metal); clipBall.position.set(0, 4.45, R * 1.14); pen.add(clipBall);
    // section (grip) and the writing end
    var section = lathe([[R * 0.62, -0.95], [R * 0.7, -0.6], [R * 0.78, -0.15], [R * 0.86, 0]], black); pen.add(section);
    if (kind === "fountain") {
      var nibShape = new T.Shape();
      nibShape.moveTo(0, -1.05); nibShape.quadraticCurveTo(0.34, -0.55, 0.3, 0.05); nibShape.lineTo(-0.3, 0.05); nibShape.quadraticCurveTo(-0.34, -0.55, 0, -1.05);
      var nib = new T.Mesh(new T.ExtrudeGeometry(nibShape, { depth: 0.035, bevelEnabled: true, bevelThickness: 0.01, bevelSize: 0.01, bevelSegments: 2 }), metal);
      nib.position.set(0, -0.85, 0.09); nib.rotation.x = -0.28; pen.add(nib);
      var slit = new T.Mesh(new T.BoxGeometry(0.012, 0.7, 0.05), black); slit.position.set(0, -1.35, 0.1); slit.rotation.x = -0.28; pen.add(slit);
      var breather = new T.Mesh(new T.CylinderGeometry(0.035, 0.035, 0.06, 16), black); breather.position.set(0, -1.02, 0.1); breather.rotation.x = Math.PI / 2 - 0.28; pen.add(breather);
      var feed = lathe([[0, -1.85], [0.12, -1.7], [0.2, -1.2], [0.24, -0.95]], black); pen.add(feed);
      anchors.nib = new T.Vector3(0.05, -1.5, 0.15);
    } else if (kind === "pencil") {
      var cone = new T.Mesh(new T.ConeGeometry(R * 0.62, 0.9, 48), metal); cone.position.y = -1.4; cone.rotation.x = Math.PI; pen.add(cone);
      var lead = new T.Mesh(new T.CylinderGeometry(0.03, 0.03, 0.35, 12), black); lead.position.y = -1.95; pen.add(lead);
      anchors.nib = new T.Vector3(0.05, -1.8, 0.1);
    } else {
      var tip = new T.Mesh(new T.ConeGeometry(R * 0.6, 0.8, 48), metal); tip.position.y = -1.35; tip.rotation.x = Math.PI; pen.add(tip);
      anchors.nib = new T.Vector3(0.05, -1.7, 0.1);
    }
    // lever on the barrel for lever fillers; a clear window for vacumatics
    var mech = (d.mechanism || "").toLowerCase();
    if (mech.indexOf("lever") >= 0) {
      var lever = new T.Mesh(new T.BoxGeometry(0.06, 1.1, 0.16), metal); lever.position.set(-R * 1.0, 1.7, 0); pen.add(lever);
      var leverRing = new T.Mesh(new T.CylinderGeometry(R * 1.01, R * 1.01, 0.05, 96, 1, true), metal); leverRing.position.y = 1.15; pen.add(leverRing);
      anchors.mech = new T.Vector3(-R * 1.05, 1.7, 0.05);
    } else if (mech.indexOf("vacumatic") >= 0 || mech.indexOf("window") >= 0) {
      var win = new T.Mesh(new T.CylinderGeometry(R * 1.005, R * 1.005, 0.5, 96, 1, true), new T.MeshPhysicalMaterial({ color: 0xc9b46a, transmission: 0.6, roughness: 0.15, thickness: 0.3 })); win.position.y = 0.55; pen.add(win);
      anchors.mech = new T.Vector3(R * 0.9, 0.55, 0.3);
    } else {
      anchors.mech = new T.Vector3(-R * 0.9, 1.9, 0.3);
    }
    anchors.cap = new T.Vector3(R * 0.9, 5.6, 0.4);

    // soft shadow under the pen
    var shCv = document.createElement("canvas"); shCv.width = 256; shCv.height = 256;
    var sg = shCv.getContext("2d"), grad = sg.createRadialGradient(128, 128, 10, 128, 128, 128);
    grad.addColorStop(0, "rgba(0,0,0,.22)"); grad.addColorStop(1, "rgba(0,0,0,0)"); sg.fillStyle = grad; sg.fillRect(0, 0, 256, 256);
    var shadow = new T.Mesh(new T.PlaneGeometry(7, 2.2), new T.MeshBasicMaterial({ map: new T.CanvasTexture(shCv), transparent: true, depthWrite: false }));
    shadow.rotation.x = -Math.PI / 2; shadow.position.set(0.3, -3.05, 0); scene.add(shadow);

    // pose: standing on its nib, leaning like the plant leans, centred a little low
    pen.position.set(0.2, -2.9, 0);
    pen.rotation.z = -0.32;
    stage.classList.add("is-3d");
    buildHotspots();
  }

  // ---- hotspots (the "+" markers with one callout, like the reference's "Wood Frame") ----------
  var spots = [];
  function buildHotspots() {
    if (!hotspotsEl) return;
    hotspotsEl.innerHTML = "";
    var list = [
      { key: "mech", label: d.mechanism || "" , show: true },
      { key: "nib", label: d.nib ? d.nib + " nib" : (d.kind === "pencil" ? "Mechanical pencil" : "") },
      { key: "cap", label: d.era ? "Made " + d.era : "" }
    ];
    list.forEach(function (it) {
      if (!anchors[it.key] || !it.label) return;
      var dot = document.createElement("span"); dot.className = "hotspot"; dot.textContent = "+";
      var lab = document.createElement("span"); lab.className = "hotspot-label"; lab.textContent = it.label;
      if (!it.show) lab.style.display = "none";
      hotspotsEl.appendChild(dot); hotspotsEl.appendChild(lab);
      spots.push({ anchor: anchors[it.key], dot: dot, label: lab });
    });
  }
  var v = new T.Vector3();
  function placeHotspots(w, h) {
    spots.forEach(function (sp) {
      v.copy(sp.anchor).applyMatrix4(pen.matrixWorld).project(camera);
      var x = (v.x + 1) / 2 * w, y = (1 - v.y) / 2 * h;
      sp.dot.style.left = x + "px"; sp.dot.style.top = y + "px";
      sp.label.style.left = (x + 64) + "px"; sp.label.style.top = y + "px";
    });
  }

  // ---- sizing, motion, visibility -------------------------------------------------------------
  var w = 1, h = 1;
  function resize() {
    var r = stage.getBoundingClientRect(); w = Math.max(1, r.width); h = Math.max(1, r.height);
    renderer.setSize(w, h, false); camera.aspect = w / h; camera.updateProjectionMatrix();
    camera.position.z = w < 520 ? 19 : (w < 700 ? 16 : 13.2);
  }
  var tilt = { x: 0, y: 0 }, target = { x: 0, y: 0 };
  if (!reduced) {
    window.addEventListener("pointermove", function (e) {
      var r = stage.getBoundingClientRect(); target.y = ((e.clientX - r.left) / r.width - 0.5) * 0.5; target.x = ((e.clientY - r.top) / r.height - 0.5) * 0.18;
    }, { passive: true });
  }
  var visible = true, raf = null, t0 = performance.now();
  if ("IntersectionObserver" in window) new IntersectionObserver(function (es) { visible = es[0].isIntersecting; if (visible && !raf) loop(); }, { threshold: 0.05 }).observe(stage);
  function frame(now) {
    var t = (now - t0) / 1000;
    if (!reduced) { tilt.x += (target.x - tilt.x) * 0.06; tilt.y += (target.y - tilt.y) * 0.06; pen.rotation.y = t * 0.35 + tilt.y; pen.rotation.x = tilt.x; }
    else { pen.rotation.y = 0.6; }
    pen.updateMatrixWorld();
    renderer.render(scene, camera);
    placeHotspots(w, h);
  }
  function loop() { raf = null; if (!visible) return; frame(performance.now()); if (!reduced) raf = requestAnimationFrame(loop); }
  window.addEventListener("resize", function () { resize(); if (reduced) frame(performance.now()); });

  samplePhoto(d.photo, function (colors) {
    if (!colors || !colors.length) colors = [new T.Color(0x1d1d21), new T.Color(0x3a3a40), new T.Color(0x101014)];
    resize();
    build(colors);
    frame(performance.now());
    if (!reduced) loop();
  });
})();
