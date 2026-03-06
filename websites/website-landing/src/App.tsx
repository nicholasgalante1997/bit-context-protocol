import { Nav } from './components/Nav';
import { Hero } from './components/Hero';
import { Features } from './components/Features';
import { CodeExample } from './components/CodeExample';
import { Ecosystem } from './components/Ecosystem';
import { Footer } from './components/Footer';

export function App() {
  return (
    <div className="min-h-screen">
      <Nav />
      <Hero />
      <Features />
      <CodeExample />
      <Ecosystem />
      <Footer />
    </div>
  );
}
