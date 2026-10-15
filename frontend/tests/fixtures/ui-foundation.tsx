import { createRoot } from "react-dom/client";
import { ThemeProvider, useTheme } from "../../src/components/theme-provider";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Card, CardHeader, CardTitle, CardDescription, CardContent, CardFooter } from "@/components/ui/card";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogTrigger } from "@/components/ui/dialog";
import "../../src/styles.css";
import "../../src/pre-match.css";
import "../../src/admin.css";
import "../../src/benchmark.css";

function Foundation() {
  const { preference, setPreference } = useTheme();
  return <main className="bg-background text-foreground">
    <Card>
      <CardHeader><CardTitle>Foundation</CardTitle><CardDescription>Token consumer</CardDescription></CardHeader>
      <CardContent>
        <FieldGroup><Field><FieldLabel htmlFor="name">Name</FieldLabel><Input id="name" /></Field></FieldGroup>
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
  </main>;
}

createRoot(document.getElementById("root")!).render(<ThemeProvider><Foundation /></ThemeProvider>);
