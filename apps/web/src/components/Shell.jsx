import { Outlet, NavLink } from "react-router-dom";
import { useTheme } from "../context/ThemeContext.jsx";

export default function Shell() {
  const { theme, toggleTheme } = useTheme();

  return (
    <div className="min-h-screen bg-base text-primary relative selection:bg-accent/20">
      {/* ── Studio Navigation Bar ───────────────────────────────────── */}
      <header className="sticky top-0 z-50 px-4 pt-3 pb-2 sm:px-6 bg-base/95 backdrop-blur-md border-b border-border">
        <nav
          className="mx-auto max-w-6xl flex items-center justify-between gap-4"
          aria-label="Studio Master Navigation"
        >
          {/* Brand Seal */}
          <NavLink to="/dashboard" className="flex items-center gap-3 group">
            <div className="w-8 h-8 rounded-lg bg-surface-elevated border border-border text-primary flex items-center justify-center font-editorial font-bold text-base shadow-sm transition-transform group-hover:scale-105 relative">
              <span className="text-accent">ד</span>
              <span className="absolute -top-0.5 -right-0.5 w-2 h-2 rounded-full bg-accent ring-1 ring-base" />
            </div>
            <div className="flex flex-col">
              <span className="font-editorial text-lg font-bold tracking-tight text-primary leading-none group-hover:text-accent transition-colors">
                DABAR
              </span>
              <span className="text-[10px] text-muted font-medium mt-0.5 tracking-wider">
                Sermon Media Studio
              </span>
            </div>
          </NavLink>

          {/* Navigation Links Dock */}
          <div className="flex items-center gap-1 bg-surface-elevated border border-border p-1 rounded-xl">
            <NavLink
              to="/dashboard"
              className={({ isActive }) =>
                `px-3.5 py-1.5 rounded-lg text-xs font-semibold tracking-tight transition-all flex items-center gap-1.5 ${
                  isActive
                    ? "bg-accent text-accent-fg shadow-xs"
                    : "text-secondary hover:text-primary hover:bg-surface-hover"
                }`
              }
            >
              <i className="bx bx-film text-sm" />
              <span>Library</span>
            </NavLink>

            <NavLink
              to="/upload"
              className={({ isActive }) =>
                `px-3.5 py-1.5 rounded-lg text-xs font-semibold tracking-tight transition-all flex items-center gap-1.5 ${
                  isActive
                    ? "bg-accent text-accent-fg shadow-xs"
                    : "text-secondary hover:text-primary hover:bg-surface-hover"
                }`
              }
            >
              <i className="bx bx-plus text-sm" />
              <span>New Sermon</span>
            </NavLink>

            <NavLink
              to="/settings"
              className={({ isActive }) =>
                `px-3.5 py-1.5 rounded-lg text-xs font-semibold tracking-tight transition-all flex items-center gap-1.5 ${
                  isActive
                    ? "bg-accent text-accent-fg shadow-xs"
                    : "text-secondary hover:text-primary hover:bg-surface-hover"
                }`
              }
            >
              <i className="bx bx-cog text-sm" />
              <span>Settings</span>
            </NavLink>
          </div>

          {/* Actions & Theme Toggle */}
          <div className="flex items-center gap-2">
            <NavLink
              to="/onboarding"
              className="hidden md:flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium text-secondary hover:text-primary bg-surface border border-border hover:border-accent-border transition-all"
              title="Overview & Feature Tour"
            >
              <i className="bx bx-compass text-accent" />
              <span>Tour</span>
            </NavLink>

            <button
              type="button"
              onClick={toggleTheme}
              className="w-8 h-8 rounded-lg bg-surface border border-border hover:border-border-strong text-secondary hover:text-primary flex items-center justify-center transition-all"
              aria-label={theme === "dark" ? "Switch to light mode" : "Switch to dark mode"}
            >
              <i className={`bx ${theme === "dark" ? "bx-sun" : "bx-moon"} text-sm`} />
            </button>
          </div>
        </nav>
      </header>

      {/* ── Main Canvas Viewport ────────────────────────────────────── */}
      <main className="py-8 px-4 sm:px-6 max-w-6xl mx-auto">
        <Outlet />
      </main>
    </div>
  );
}
