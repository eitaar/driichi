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
} as const;

export type TableTextureKey = keyof typeof TABLE_TEXTURE_URLS;

// Keep the felt readable as a deep green with or without the local texture.
// The texture remains the original asset; this color is only the material tint.
export const FELT_MATERIAL_TINT = "#225d44";

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
    repeat: [1.5, 1],
    minFilter: "mipmap",
    colorSpace: "srgb",
    anisotropy: 1,
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
