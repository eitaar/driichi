import { describe, expect, it } from "vitest";
import {
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
  it("keeps all approved local table materials in the bundle", () => {
    expect(TABLE_TEXTURE_URLS.felt).toContain("table-felt.webp");
    expect(TABLE_TEXTURE_URLS.rail).toContain("table-rail.webp");
    expect(TABLE_TEXTURE_URLS.center).toContain("center-device.webp");
    expect(TABLE_TEXTURE_URLS.back).toContain("tile-back-material.webp");
  });

  it("defines restrained color-space, filtering, and wrapping per material", () => {
    expect(TABLE_TEXTURE_SPECS.felt.wrap).toBe("repeat");
    expect(TABLE_TEXTURE_SPECS.rail.wrap).toBe("repeat");
    expect(TABLE_TEXTURE_SPECS.center.wrap).toBe("clamp");
    expect(TABLE_TEXTURE_SPECS.back.wrap).toBe("clamp");
    expect(TABLE_TEXTURE_SPECS.felt.colorSpace).toBe("srgb");
    expect(TABLE_TEXTURE_SPECS.back.minFilter).toBe("linear");
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
    disposeTableTextures({ felt: { dispose } as never, back: { dispose } as never });
    expect(disposeCalls).toBe(2);
  });
});
