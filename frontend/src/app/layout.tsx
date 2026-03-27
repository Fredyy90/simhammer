import type { Metadata } from 'next';
import Script from 'next/script';
import DesktopAppLink from './components/DesktopAppLink';
import SettingsPopover from './components/SettingsPopover';
import { SimProvider } from './components/SimContext';
import SimSharedConfig from './components/SimSharedConfig';
import SimTypeCards from './components/SimTypeCards';
import UpdateChecker from './components/UpdateChecker';
import WindowControls from './components/WindowTitlebar';
import './globals.css';
import packageJson from '../../package.json';

export const metadata: Metadata = {
  title: 'SimHammer',
  description: 'Run SimulationCraft simulations from your browser',
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <head>
        <script
          dangerouslySetInnerHTML={{
            __html: `if(window.electronAPI)document.documentElement.setAttribute("data-desktop","")`,
          }}
        />
        <Script
          id="wowhead-config"
          strategy="afterInteractive"
        >{`const whTooltips = { colorLinks: false, iconizeLinks: false, renameLinks: false };`}</Script>
        <Script src="https://wow.zamimg.com/js/tooltips.js" strategy="afterInteractive" />
      </head>
      <body className="min-h-screen">
        <UpdateChecker />
        <SimProvider>
          <header className="desktop-drag sticky top-0 z-50 border-b border-border bg-bg/80 backdrop-blur-xl">
            <div className="flex h-12 items-center justify-between px-6">
              <a
                href="https://simhammer.com"
                target="_blank"
                rel="noopener noreferrer"
                className="desktop-no-drag group flex items-center gap-2"
              >
                <div className="flex h-5 w-5 items-center justify-center rounded bg-gold/90">
                  <svg className="h-3 w-3 text-black" viewBox="0 0 16 16" fill="currentColor">
                    <path d="M3 2l10 6-10 6V2z" />
                  </svg>
                </div>
                <span className="text-sm font-semibold text-gray-200 transition-colors group-hover:text-white">
                  SimHammer
                </span>
              </a>
              <div className="desktop-no-drag flex items-center gap-2">
                <SettingsPopover />
                <DesktopAppLink />
                <WindowControls />
              </div>
            </div>
          </header>
          <main className="mx-auto max-w-5xl px-6 py-10">
            <SimTypeCards />
            <SimSharedConfig />
            {children}
          </main>
        </SimProvider>
        <footer className="mt-16 border-t border-border/50 py-6">
          <p className="mx-auto max-w-lg text-center text-[11px] leading-relaxed text-gray-500">
            SimHammer is a pet project held together by coffee, duct tape, and prayers to the RNG
            gods. Bugs are not features — but they might sim higher than your gear. Use at your own
            risk. Not affiliated with Blizzard, Raidbots, or anyone who knows what they&apos;re
            doing.
          </p>
          <p className="mt-2 text-center text-[11px] text-gray-600">v{packageJson.version}</p>
        </footer>
      </body>
    </html>
  );
}
