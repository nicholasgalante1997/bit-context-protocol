import { useRef, useMemo } from 'react';
import { Canvas, useFrame } from '@react-three/fiber';
import { Float } from '@react-three/drei';
import * as THREE from 'three';

function DataStream({ count = 120 }: { count?: number }) {
  const mesh = useRef<THREE.InstancedMesh>(null!);
  const dummy = useMemo(() => new THREE.Object3D(), []);

  const particles = useMemo(() => {
    return Array.from({ length: count }, () => ({
      x: (Math.random() - 0.5) * 8,
      y: (Math.random() - 0.5) * 6,
      z: (Math.random() - 0.5) * 4,
      speed: 0.2 + Math.random() * 0.6,
      phase: Math.random() * Math.PI * 2,
      scale: 0.02 + Math.random() * 0.04
    }));
  }, [count]);

  useFrame(({ clock }) => {
    const t = clock.getElapsedTime();
    particles.forEach((p, i) => {
      const y = ((p.y + t * p.speed) % 6) - 3;
      dummy.position.set(
        p.x + Math.sin(t * 0.5 + p.phase) * 0.3,
        y,
        p.z
      );
      dummy.scale.setScalar(p.scale * (1 + Math.sin(t * 2 + p.phase) * 0.3));
      dummy.updateMatrix();
      mesh.current.setMatrixAt(i, dummy.matrix);
    });
    mesh.current.instanceMatrix.needsUpdate = true;
  });

  return (
    <instancedMesh ref={mesh} args={[undefined, undefined, count]}>
      <sphereGeometry args={[1, 8, 8]} />
      <meshBasicMaterial color="#38bdf8" transparent opacity={0.6} />
    </instancedMesh>
  );
}

function WireBlock() {
  const ref = useRef<THREE.Group>(null!);

  useFrame(({ clock }) => {
    const t = clock.getElapsedTime();
    ref.current.rotation.x = Math.sin(t * 0.3) * 0.1;
    ref.current.rotation.y = t * 0.15;
  });

  return (
    <Float speed={1.5} rotationIntensity={0.2} floatIntensity={0.5}>
      <group ref={ref}>
        {/* Outer wireframe cube */}
        <mesh>
          <boxGeometry args={[2.2, 2.2, 2.2]} />
          <meshBasicMaterial color="#38bdf8" wireframe transparent opacity={0.15} />
        </mesh>

        {/* Inner solid blocks representing data segments */}
        {[
          { pos: [0, 0.6, 0] as const, size: [1.6, 0.3, 1.6] as const, color: '#0ea5e9' },
          { pos: [0, 0, 0] as const, size: [1.6, 0.3, 1.6] as const, color: '#38bdf8' },
          { pos: [0, -0.6, 0] as const, size: [1.6, 0.3, 1.6] as const, color: '#7dd3fc' }
        ].map((block, i) => (
          <mesh key={i} position={block.pos}>
            <boxGeometry args={block.size} />
            <meshBasicMaterial color={block.color} transparent opacity={0.25} />
          </mesh>
        ))}

        {/* Center glow */}
        <mesh>
          <sphereGeometry args={[0.3, 16, 16]} />
          <meshBasicMaterial color="#38bdf8" transparent opacity={0.4} />
        </mesh>
      </group>
    </Float>
  );
}

export function HeroScene() {
  return (
    <div className="absolute inset-0 pointer-events-none">
      <Canvas
        camera={{ position: [0, 0, 6], fov: 45 }}
        dpr={[1, 1.5]}
        gl={{ antialias: true, alpha: true }}
        style={{ background: 'transparent' }}
      >
        <WireBlock />
        <DataStream />
      </Canvas>
    </div>
  );
}
