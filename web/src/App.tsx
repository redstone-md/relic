import { Redirect, Route, Switch } from "wouter";

import { ErrorNote, Loading } from "@/components/common";
import { Shell } from "@/components/shell";
import { useI18n } from "@/lib/i18n";
import { useLiveEvents, useMe, useSetupStatus } from "@/lib/queries";
import { Login, Setup } from "@/pages/auth";
import { Dashboard } from "@/pages/dashboard";
import { InstancePage } from "@/pages/instance";
import { NewInstancePage } from "@/pages/instance-new";
import { NewsPage } from "@/pages/news";
import { PlayersPage } from "@/pages/players";
import { SettingsPage } from "@/pages/settings";

export function App() {
  const setup = useSetupStatus();
  const me = useMe();
  const { t } = useI18n();
  useLiveEvents(Boolean(me.data));

  if (setup.isPending || me.isPending) {
    return (
      <div className="grid min-h-dvh place-items-center">
        <Loading label={t("Connecting")} />
      </div>
    );
  }
  const error = setup.error ?? me.error;
  if (error) {
    return (
      <div className="mx-auto grid min-h-dvh max-w-md place-items-center p-6">
        <ErrorNote error={error} />
      </div>
    );
  }
  if (setup.data?.needed) return <Setup />;
  if (!me.data) return <Login />;

  return (
    <Shell user={me.data}>
      <Switch>
        <Route path="/" component={Dashboard} />
        <Route path="/instances/new" component={NewInstancePage} />
        <Route path="/instances/:id/:tab?">
          {(params) => <InstancePage key={params.id} id={params.id} tab={params.tab} />}
        </Route>
        <Route path="/news" component={NewsPage} />
        <Route path="/players" component={PlayersPage} />
        <Route path="/settings" component={SettingsPage} />
        <Route>
          <Redirect to="/" />
        </Route>
      </Switch>
    </Shell>
  );
}
