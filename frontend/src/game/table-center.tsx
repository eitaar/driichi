import { useEffect } from "react";
import { useThree } from "@react-three/fiber";
import type { Texture } from "three";

export function TableCenter({ texture }: { texture: Texture | null }) {
  const invalidate = useThree((state) => state.invalidate);
  useEffect(() => { invalidate(); }, [texture, invalidate]);
  return texture ? (
    <mesh name="webgl-center-display" position={[0, 0.47, 0]} rotation={[-Math.PI / 2, 0, 0]}>
      <planeGeometry args={[3.05, 2.52]} />
      <meshBasicMaterial map={texture} toneMapped={false} />
    </mesh>
  ) : null;
}
