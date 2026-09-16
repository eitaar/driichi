import { useEffect, useRef } from "react";

const TILE_SOURCES = [
  new URL("./assets/tiles/Regular/Man1.svg", import.meta.url).href,
  new URL("./assets/tiles/Regular/Pin5.svg", import.meta.url).href,
  new URL("./assets/tiles/Regular/Sou3.svg", import.meta.url).href,
  new URL("./assets/tiles/Regular/Ton.svg", import.meta.url).href,
  new URL("./assets/tiles/Regular/Haku.svg", import.meta.url).href,
  new URL("./assets/tiles/Regular/Chun.svg", import.meta.url).href,
];

export function TileVignette() {
  const hostRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const element = hostRef.current as HTMLDivElement | undefined;
    if (element === undefined) return;
    let disposed = false;
    let app: import("pixi.js").Application | undefined;
    let stopMotion: () => void = () => undefined;
    let observer: ResizeObserver | undefined;

    async function mount() {
      try {
        const { Application, Assets, Sprite } = await import("pixi.js");
        if (disposed) return;
        app = new Application();
        await app.init({
          antialias: true,
          autoDensity: true,
          backgroundAlpha: 0,
          preference: "webgl",
          resizeTo: element,
          resolution: Math.min(window.devicePixelRatio || 1, 2),
        });
        if (disposed) {
          app.destroy({ removeView: true }, { children: true });
          app = undefined;
          return;
        }

        app.stage.eventMode = "none";
        app.canvas.setAttribute("aria-hidden", "true");
        app.canvas.className = "vignette-canvas";
        element!.appendChild(app.canvas);

        const textures = await Promise.all(TILE_SOURCES.map((source) => Assets.load(source)));
        if (disposed) return;
        const sprites = textures.map((texture, index) => {
          const sprite = new Sprite(texture);
          sprite.anchor.set(0.5);
          sprite.alpha = index === 3 ? 0.72 : 0.94;
          sprite.tint = index === 3 ? 0xb93a35 : 0xf2f0e9;
          sprite.eventMode = "none";
          app!.stage.addChild(sprite);
          return sprite;
        });
        const positions = [
          [0.24, 0.52, -0.14, 0.24],
          [0.42, 0.27, 0.1, 0.28],
          [0.62, 0.67, 0.12, 0.22],
          [0.78, 0.35, -0.1, 0.25],
          [0.9, 0.68, 0.08, 0.2],
          [0.56, 0.2, -0.08, 0.18],
        ];
        const layout = () => {
          const { width, height } = app!.screen;
          const scale = Math.min(width / 1100, height / 640);
          sprites.forEach((sprite, index) => {
            const [x, y, rotation, size] = positions[index];
            sprite.x = width * x;
            sprite.y = height * y;
            sprite.rotation = rotation;
            sprite.scale.set(scale * size);
          });
        };
        layout();
        observer = new ResizeObserver(layout);
        observer.observe(element!);

        if (!window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
          let elapsed = 0;
          const tick = (ticker: { deltaTime: number }) => {
            elapsed += ticker.deltaTime * 0.002;
            sprites.forEach((sprite, index) => {
              sprite.y += Math.sin(elapsed + index) * 0.04;
              sprite.rotation += (index % 2 === 0 ? 1 : -1) * 0.00035;
            });
          };
          app.ticker.add(tick);
          stopMotion = () => { app?.ticker.remove(tick); };
        }
      } catch {
        if (!disposed) element!.dataset.fallback = "true";
      }
    }

    void mount();
    return () => {
      disposed = true;
      observer?.disconnect();
      stopMotion();
      app?.destroy({ removeView: true }, { children: true });
      element!.replaceChildren();
    };
  }, []);

  return <div ref={hostRef} className="tile-vignette" aria-hidden="true" data-testid="tile-vignette" />;
}
