<!-- Says that Dea follows the project's files on disk (F1-FON-05). -->
<script lang="ts">
  import { projectApi } from "$app/contributions/project-api";

  const project = projectApi();
  const watching = $derived(project.info !== null && !project.info.missing);
</script>

{#if watching}<span>Project files are watched for changes</span>{/if}
