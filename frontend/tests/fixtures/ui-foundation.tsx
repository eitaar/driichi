/// <reference types="vite/client" />

import { createRoot } from "react-dom/client";
import { AppShell } from "../../src/components/app-shell";
import { BenchmarkCharts } from "../../src/benchmark-charts";
import { ThemeProvider, useTheme } from "../../src/components/theme-provider";
import { Button } from "../../src/components/ui/button";
import { Input } from "../../src/components/ui/input";
import { Checkbox } from "../../src/components/ui/checkbox";
import { Select, SelectTrigger, SelectValue, SelectContent, SelectGroup, SelectItem } from "../../src/components/ui/select";
import { ToggleGroup, ToggleGroupItem } from "../../src/components/ui/toggle-group";
import { Field, FieldGroup, FieldLabel, FieldDescription, FieldError } from "../../src/components/ui/field";
import { Card, CardHeader, CardTitle, CardDescription, CardContent, CardFooter } from "../../src/components/ui/card";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogTrigger } from "../../src/components/ui/dialog";
import "../../src/styles.css";
import "../../src/pre-match.css";
import "../../src/admin.css";
import "../../src/benchmark.css";

const participants = [{ value: "amber", label: "Amber" }, { value: "birch", label: "Birch" }];

function Foundation() {
  const { preference, setPreference } = useTheme();
  return <><AppShell variant="public">{null}</AppShell><p id="inherited-label">Inherited body label</p><main id="foundation-preview" className="bg-background text-foreground">
    <Card>
      <CardHeader><CardTitle>Foundation</CardTitle><CardDescription>Token consumer</CardDescription></CardHeader>
      <CardContent>
        <FieldGroup>
          <Field><FieldLabel htmlFor="name">Name</FieldLabel><Input id="name" placeholder="Participant name" aria-describedby="name-help" /><FieldDescription id="name-help">Shown to the Room.</FieldDescription></Field>
          <Field><FieldLabel htmlFor="participant">Participant</FieldLabel><Select defaultValue="amber" items={participants}><SelectTrigger id="participant"><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value="amber">Amber</SelectItem><SelectItem value="birch">Birch</SelectItem></SelectGroup></SelectContent></Select></Field>
          <Field><FieldLabel htmlFor="locked-participant">Locked Participant</FieldLabel><Select defaultValue="amber" items={participants} disabled><SelectTrigger id="locked-participant"><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value="amber">Amber</SelectItem></SelectGroup></SelectContent></Select></Field>
          <Select defaultValue="amber" items={participants}><SelectTrigger size="sm" aria-label="Compact Participant"><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value="amber">Amber</SelectItem></SelectGroup></SelectContent></Select>
          <Field orientation="horizontal"><Checkbox id="ready" /><FieldLabel htmlFor="ready">Ready for next Match</FieldLabel></Field>
          <Field orientation="horizontal"><Checkbox id="disabled-ready" disabled defaultChecked /><FieldLabel htmlFor="disabled-ready">Ready unavailable</FieldLabel></Field>
          <Field><FieldLabel htmlFor="empty-name">Empty name</FieldLabel><Input id="empty-name" placeholder="Participant name" /></Field>
          <Field><FieldLabel htmlFor="locked-name">Locked name</FieldLabel><Input id="locked-name" defaultValue="Amber" disabled /></Field>
          <Field data-invalid><FieldLabel htmlFor="invalid-name">Invalid name</FieldLabel><Input id="invalid-name" defaultValue="!" aria-invalid aria-describedby="name-error" /><FieldError id="name-error">Enter a display name.</FieldError></Field>
        </FieldGroup>
        <div className="flex flex-wrap gap-2">
          <Button>Normal action</Button><Button disabled aria-busy="true">Pending action</Button><Button disabled>Disabled action</Button>
          <Button variant="destructive" disabled>Disabled destructive action</Button>
          <Button size="lg">Large action</Button><Button size="sm">Small action</Button><Button size="icon-sm" aria-label="Compact action">+</Button>
        </div>
        <output aria-label="Theme preference">{preference}</output>
        <ToggleGroup orientation="vertical" aria-label="Text alignment">
          <ToggleGroupItem value="left">Align left</ToggleGroupItem>
          <ToggleGroupItem value="center">Align center</ToggleGroupItem>
          <ToggleGroupItem value="right">Align right</ToggleGroupItem>
        </ToggleGroup>
      </CardContent>
      <CardFooter>
        <Button onClick={() => setPreference("light")}>Light</Button>
        <Button onClick={() => setPreference("dark")}>Dark</Button>
        <Button onClick={() => setPreference("system")}>System</Button>
        <Dialog>
          <DialogTrigger render={<Button variant="outline" />}>Open dialog</DialogTrigger>
          <DialogContent><DialogHeader><DialogTitle>Accessible overlay</DialogTitle><DialogDescription>Keyboard focus stays inside.</DialogDescription></DialogHeader><Input aria-label="Dialog name" /></DialogContent>
        </Dialog>
      </CardFooter>
    </Card>
    <BenchmarkCharts analytics={{ kind: "ready", completed: 1, provisional: true, participants: ["Akari", "Nori", "Ren", "Aya"].map((displayName, i) => ({ participantId: `p${i}`, displayName, averageRank: i + 1, firstPlaceRate: Number(i === 0), cumulativeNet: [300, 100, -100, -300][i], rankCounts: [0, 1, 2, 3].map(rank => Number(rank === i)), points: [{ sequence: 1, matchId: "fixture-match", net: [300, 100, -100, -300][i], cumulative: [300, 100, -100, -300][i], movingRank: i + 1, movingN: 1 }] })) }} onOpenMatch={() => {}} />
  </main></>;
}

createRoot(document.getElementById("root")!).render(<ThemeProvider><Foundation /></ThemeProvider>);
