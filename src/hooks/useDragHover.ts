// Core
import { useEffect, useState } from "react";
// Services
import fileforgeApi from "@/services/fileforgeApi";

/** `true` while files are dragged over the window and `active` is set. */
export function useDragHover(active: boolean): boolean {
  const [hovering, setHovering] = useState(false);

  useEffect(() => {
    if (!active) {
      setHovering(false);
      return;
    }
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void fileforgeApi.onDragHover(setHovering).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [active]);

  return hovering;
}

export default useDragHover;
