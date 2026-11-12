import { createContext, type ReactNode, useContext, useEffect, useMemo, useState } from "react";

type Choice = "light" | "dark" | "system";

interface ThemeState {
  choice: Choice;
  resolved: "light" | "dark";
  set: (choice: Choice) => void;
}

const KEY = "relic.theme";
const Context = createContext<ThemeState | null>(null);

function stored(): Choice {
  try {
    const value = localStorage.getItem(KEY);
    if (value === "light" || value === "dark" || value === "system") return value;
  } catch {
    // Storage may be blocked; fall through to the default.
  }
  // The panel is drawn dark first; light is a choice.
  return "dark";
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [choice, setChoice] = useState<Choice>(stored);
  const [systemDark, setSystemDark] = useState(
    () => matchMedia("(prefers-color-scheme: dark)").matches,
  );

  useEffect(() => {
    const media = matchMedia("(prefers-color-scheme: dark)");
    const listen = (event: MediaQueryListEvent) => setSystemDark(event.matches);
    media.addEventListener("change", listen);
    return () => media.removeEventListener("change", listen);
  }, []);

  const resolved = choice === "system" ? (systemDark ? "dark" : "light") : choice;

  useEffect(() => {
    document.documentElement.classList.toggle("dark", resolved === "dark");
  }, [resolved]);

  const value = useMemo<ThemeState>(
    () => ({
      choice,
      resolved,
      set: (next) => {
        setChoice(next);
        try {
          localStorage.setItem(KEY, next);
        } catch {
          // Not remembered, still applied.
        }
      },
    }),
    [choice, resolved],
  );

  return <Context.Provider value={value}>{children}</Context.Provider>;
}

export function useTheme(): ThemeState {
  const value = useContext(Context);
  if (!value) throw new Error("useTheme outside ThemeProvider");
  return value;
}
