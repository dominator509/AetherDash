import { useEffect, useRef } from "react";
import { useFocusVisible } from "./lib/focus";
import { bootstrap } from "./lib/bootstrap";
import { AppFrame } from "./components/shell/AppFrame";
import { WsErrorOverlay } from "./components/states/WsErrorOverlay";

export default function App() {
  useFocusVisible();

  // Bootstrap ref so the effect runs once per mount
  const cleanupRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    bootstrap()
      .then((cleanup) => {
        cleanupRef.current = cleanup;
      })
      .catch((err: unknown) => {
        console.error("[app] bootstrap failed:", err);
      });

    return () => {
      cleanupRef.current?.();
      cleanupRef.current = null;
    };
  }, []);

  return (
    <>
      <AppFrame />
      <WsErrorOverlay />
    </>
  );
}
