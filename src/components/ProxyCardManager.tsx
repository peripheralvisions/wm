import React, { useEffect, useState } from 'react';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

interface ProxyCardPayload {
  hwnd: number;
  action: 'show' | 'update' | 'hide';
  x: number;
  y: number;
  width: number;
  height: number;
}

interface ProxyCardState {
  hwnd: number;
  x: number;
  y: number;
  width: number;
  height: number;
}

export function ProxyCardManager() {
  const [cards, setCards] = useState<Map<number, ProxyCardState>>(new Map());

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;

    const setupListener = async () => {
      unlisten = await listen<ProxyCardPayload>('proxy-card', (event) => {
        const payload = event.payload;

        setCards((prevCards) => {
          const newCards = new Map(prevCards);

          if (payload.action === 'show') {
            // Initial mount. Dimensions might be 0 until the first update.
            newCards.set(payload.hwnd, {
              hwnd: payload.hwnd,
              x: payload.x,
              y: payload.y,
              width: payload.width,
              height: payload.height,
            });
          } else if (payload.action === 'update') {
            // High-frequency 144Hz updates
            if (newCards.has(payload.hwnd)) {
              newCards.set(payload.hwnd, {
                hwnd: payload.hwnd,
                x: payload.x,
                y: payload.y,
                width: payload.width,
                height: payload.height,
              });
            }
          } else if (payload.action === 'hide') {
            // Unmount when scroll is committed
            newCards.delete(payload.hwnd);
          }

          return newCards;
        });
      });
    };

    setupListener();

    return () => {
      if (unlisten) {
        unlisten();
      }
    };
  }, []);

  return (
    <>
      {Array.from(cards.values()).map((card) => (
        <div
          key={card.hwnd}
          style={{
            position: 'fixed',
            top: 0,
            left: 0,
            width: `${card.width}px`,
            height: `${card.height}px`,
            // Hardware accelerated CSS transform for smooth 144Hz movement
            transform: `translate3d(${card.x}px, ${card.y}px, 0)`,
            willChange: 'transform',
            backgroundColor: 'rgba(30, 30, 30, 0.85)',
            border: '2px solid rgba(255, 255, 255, 0.2)',
            borderRadius: '8px',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            color: 'white',
            fontWeight: 'bold',
            fontSize: '24px',
            zIndex: 9999, // Ensure it's on top
            pointerEvents: 'none', // Ignore pointer events so they pass through
            opacity: card.width === 0 ? 0 : 1,
            transition: 'opacity 0.2s ease-in-out',
          }}
        >
          🎮 Game Proxy (ID: {card.hwnd})
        </div>
      ))}
    </>
  );
}
