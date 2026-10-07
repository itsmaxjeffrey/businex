import React, { useCallback, useRef } from "react";
import { useWindows, type WindowState } from "../lib/store";

export function WindowFrame({ win, children }: { win: WindowState; children: React.ReactNode }) {
  const { close, focus, setBounds, minimize, toggleMaximize } = useWindows();
  const dragRef = useRef<{ startX: number; startY: number; x: number; y: number } | null>(null);
  const resizeRef = useRef<{ startX: number; startY: number; w: number; h: number } | null>(null);

  const onTitlePointerDown = useCallback((e: React.PointerEvent) => {
    if (win.maximized) return;
    focus(win.id);
    dragRef.current = { startX: e.clientX, startY: e.clientY, x: win.x, y: win.y };
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  }, [focus, win]);

  const onTitlePointerMove = useCallback((e: React.PointerEvent) => {
    const d = dragRef.current;
    if (!d) return;
    setBounds(win.id, {
      x: Math.max(8, d.x + (e.clientX - d.startX)),
      y: Math.max(48, d.y + (e.clientY - d.startY)),
    });
  }, [setBounds, win.id]);

  const onTitlePointerUp = useCallback(() => { dragRef.current = null; }, []);

  const onResizePointerDown = useCallback((e: React.PointerEvent) => {
    e.stopPropagation();
    focus(win.id);
    resizeRef.current = { startX: e.clientX, startY: e.clientY, w: win.w, h: win.h };
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  }, [focus, win]);

  const onResizePointerMove = useCallback((e: React.PointerEvent) => {
    const r = resizeRef.current;
    if (!r) return;
    setBounds(win.id, {
      w: Math.max(420, r.w + (e.clientX - r.startX)),
      h: Math.max(300, r.h + (e.clientY - r.startY)),
    });
  }, [setBounds, win.id]);

  const onResizePointerUp = useCallback(() => { resizeRef.current = null; }, []);

  const style: React.CSSProperties = win.maximized
    ? { left: 64, top: 52, width: "calc(100vw - 128px)", height: "calc(100vh - 128px)", zIndex: win.z }
    : { left: win.x, top: win.y, width: win.w, height: win.h, zIndex: win.z };

  return (
    <section
      className="window-frame panel"
      style={style}
      onPointerDown={() => focus(win.id)}
      aria-label={win.title}
    >
      <header
        className="window-titlebar"
        onPointerDown={onTitlePointerDown}
        onPointerMove={onTitlePointerMove}
        onPointerUp={onTitlePointerUp}
        onDoubleClick={() => toggleMaximize(win.id)}
      >
        <div className="flex items-center gap-2">
          <button className="win-dot" style={{ background: "#ff5f57" }} onClick={() => close(win.id)} aria-label="Close" />
          <button className="win-dot" style={{ background: "#febc2e" }} onClick={() => minimize(win.id)} aria-label="Minimize" />
          <button className="win-dot" style={{ background: "#28c840" }} onClick={() => toggleMaximize(win.id)} aria-label="Maximize" />
        </div>
        <div className="font-display ml-2 text-sm tracking-wide">{win.title}</div>
        <div className="ml-auto text-[11px] opacity-40 font-mono">{win.module}</div>
      </header>

      <div className="relative flex-1 overflow-hidden">
        {children}
      </div>

      {!win.maximized && (
        <div
          className="absolute bottom-0 right-0 h-4 w-4 cursor-nwse-resize"
          onPointerDown={onResizePointerDown}
          onPointerMove={onResizePointerMove}
          onPointerUp={onResizePointerUp}
          style={{ background: "linear-gradient(135deg, transparent 50%, rgba(255,107,53,0.5) 50%)" }}
        />
      )}
    </section>
  );
}
