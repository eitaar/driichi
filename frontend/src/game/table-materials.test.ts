/// <reference types="node" />
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  FELT_MATERIAL_TINT,
  TABLE_TEXTURE_URLS,
  TABLE_TEXTURE_SPECS,
  configureTableTexture,
  disposeTableTextures,
} from "./table-materials";

function textureStub() {
  return {
    colorSpace: "",
    minFilter: 0,
    magFilter: 0,
    wrapS: 0,
    wrapT: 0,
    generateMipmaps: false,
    anisotropy: 0,
    repeat: { set: (x: number, y: number) => Object.assign(texture, { repeatX: x, repeatY: y }) },
    needsUpdate: false,
  } as unknown as Parameters<typeof configureTableTexture>[0] & {
    repeatX?: number;
    repeatY?: number;
  };
}

const texture = textureStub();

describe("table material assets", () => {
  it("loads only the sampled felt texture", () => {
    expect(Object.keys(TABLE_TEXTURE_URLS)).toEqual(["felt"]);
    expect(TABLE_TEXTURE_URLS.felt).toContain("table-felt.webp");
  });

  it("defines restrained color-space, filtering, and wrapping for the sampled material", () => {
    expect(TABLE_TEXTURE_SPECS.felt.wrap).toBe("repeat");
    expect(TABLE_TEXTURE_SPECS.felt.repeat).toEqual([1.5, 1]);
    expect(TABLE_TEXTURE_SPECS.felt.colorSpace).toBe("srgb");
    expect(TABLE_TEXTURE_SPECS.felt.minFilter).toBe("mipmap");
  });

  it("keeps felt direct-loaded and deep-green without a derived canvas texture", () => {
    const sceneSource = readFileSync(
      resolve(process.cwd(), "src/game/three-table-scene.tsx"),
      "utf8",
    );

    expect(FELT_MATERIAL_TINT).toBe("#225d44");
    expect(sceneSource).not.toContain("CanvasTexture");
    expect(sceneSource).not.toContain("tintFeltTexture");
    expect(sceneSource).toContain(
      "(texture) => resolve(configureTableTexture(texture, TABLE_TEXTURE_SPECS[key]))",
    );
    expect(sceneSource).not.toMatch(
      /<meshBasicMaterial\s+color="#ffffff"\s+map=\{texture\}/,
    );
  });

  it("applies texture sampling settings without relying on defaults", () => {
    configureTableTexture(texture, TABLE_TEXTURE_SPECS.felt);
    expect(texture.colorSpace).toBe("srgb");
    expect(texture.wrapS).not.toBe(0);
    expect(texture.wrapT).not.toBe(0);
    expect(texture.magFilter).not.toBe(0);
    expect(texture.needsUpdate).toBe(true);
  });

  it("disposes every owned texture exactly once", () => {
    const dispose = () => { disposeCalls += 1; };
    let disposeCalls = 0;
    disposeTableTextures({ felt: { dispose } as never });
    expect(disposeCalls).toBe(1);
  });
});
