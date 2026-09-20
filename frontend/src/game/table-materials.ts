import {
  ClampToEdgeWrapping,
  LinearFilter,
  LinearMipmapLinearFilter,
  RepeatWrapping,
  SRGBColorSpace,
  type Texture,
} from "three";

export const TABLE_TEXTURE_URLS = {
  felt: new URL("../assets/table/table-felt.webp", import.meta.url).href,
  rail: new URL("../assets/table/table-rail.webp", import.meta.url).href,
  center: new URL("../assets/table/center-device.webp", import.meta.url).href,
  back: new URL("../assets/table/tile-back-material.webp", import.meta.url).href,
} as const;

export type TableTextureKey = keyof typeof TABLE_TEXTURE_URLS;

export interface TableTextureSpec {
  readonly wrap: "repeat" | "clamp";
  readonly repeat: readonly [number, number];
  readonly minFilter: "linear" | "mipmap";
  readonly colorSpace: "srgb";
  readonly anisotropy: number;
}

export const TABLE_TEXTURE_SPECS: Record<TableTextureKey, TableTextureSpec> = {
  felt: {
    wrap: "repeat",
    repeat: [4, 3],
    minFilter: "mipmap",
    colorSpace: "srgb",
    anisotropy: 4,
  },
  rail: {
    wrap: "repeat",
    repeat: [3, 1],
    minFilter: "mipmap",
    colorSpace: "srgb",
    anisotropy: 4,
  },
  center: {
    wrap: "clamp",
    repeat: [1, 1],
    minFilter: "linear",
    colorSpace: "srgb",
    anisotropy: 2,
  },
  back: {
    wrap: "clamp",
    repeat: [1, 1],
    minFilter: "linear",
    colorSpace: "srgb",
    anisotropy: 2,
  },
};

export function configureTableTexture(
  texture: Texture,
  spec: TableTextureSpec,
): Texture {
  texture.colorSpace = SRGBColorSpace;
  texture.wrapS = spec.wrap === "repeat" ? RepeatWrapping : ClampToEdgeWrapping;
  texture.wrapT = spec.wrap === "repeat" ? RepeatWrapping : ClampToEdgeWrapping;
  texture.minFilter = spec.minFilter === "mipmap"
    ? LinearMipmapLinearFilter
    : LinearFilter;
  texture.magFilter = LinearFilter;
  texture.generateMipmaps = spec.minFilter === "mipmap";
  texture.anisotropy = spec.anisotropy;
  texture.repeat.set(...spec.repeat);
  texture.needsUpdate = true;
  return texture;
}

export function disposeTableTextures(
  textures: Partial<Record<TableTextureKey, Texture>> | null,
): void {
  if (!textures) return;
  for (const texture of Object.values(textures)) texture?.dispose();
}
