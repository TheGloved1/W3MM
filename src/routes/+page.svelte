<script lang="ts">
  import PageHeader from "$lib/components/page-header.svelte";
  import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "$lib/components/ui/card";
  import { Button } from "$lib/components/ui/button";
  const appVersion = __APP_VERSION__;
</script>

<div class="flex flex-1 flex-col min-w-0 bg-background overflow-hidden">
  <PageHeader title="Home" subtitle="Welcome to your new app" />

  <div class="flex-1 overflow-auto p-6">
    <div class="mx-auto max-w-[720px] space-y-6">
      <Card>
        <CardHeader>
          <CardTitle class="text-sm">Getting started</CardTitle>
          <CardDescription>
            Template is wired up (v{appVersion}). Make it yours, then delete this card.
          </CardDescription>
        </CardHeader>
        <CardContent class="space-y-2 text-sm">
          <ol class="list-decimal ml-5 space-y-1 text-muted-foreground">
            <li>Follow the rename checklist in <span class="font-mono">README.md</span>.</li>
            <li>Add pages in <span class="font-mono">src/routes/</span> + nav entries in <span class="font-mono">src/routes/+layout.svelte</span>.</li>
            <li>Add backend commands in <span class="font-mono">src-tauri/src/lib.rs</span> and call them via <span class="font-mono">invoke</span>.</li>
            <li>Pick a theme in Settings — shared <span class="font-mono">PageHeader</span> / <span class="font-mono">DataList</span> components keep new pages consistent.</li>
          </ol>
          <div class="pt-2">
            <Button size="sm" href="/settings">Open Settings</Button>
          </div>
        </CardContent>
      </Card>
    </div>
  </div>
</div>
