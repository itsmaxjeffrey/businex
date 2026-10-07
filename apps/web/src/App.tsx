import React from "react";
import { useSession } from "./lib/store";
import { AuthScreen } from "./auth/AuthScreen";
import { Desktop } from "./desktop/Desktop";
import { Spinner } from "./components/ui";

export function App() {
  const { user, loading } = useSession();

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center">
        <Spinner label="Waking up your workspace…" />
      </div>
    );
  }

  return user ? <Desktop /> : <AuthScreen />;
}
