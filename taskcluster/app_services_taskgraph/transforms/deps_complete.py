from taskgraph.transforms.base import TransformSequence
transforms = TransformSequence()

@transforms.add
def deps_complete_script(config, tasks):
    """Setup the deps-complete.py script"""
    for task in tasks:
        task.update({
            # Run this task when all dependencies are completed, rather than
            # requiring them to be successful
            'requires': 'all-resolved',
            'worker-type': 'b-linux',
            'worker': {
                'chain-of-trust': True,
                'docker-image': { 'in-tree': 'linux' },
                'max-run-time': 1800,
                'env': {
                    'DECISION_TASK_ID': {
                        'task-reference': '<decision>'
                    },
                    'TASK_ID': {
                        'task-reference': '<self>'
                    },
                },
            },
            'run': {
                'using': 'run-task',
                'command': '/builds/worker/checkouts/vcs/taskcluster/scripts/deps-complete.py',
            }
        })
        yield task

@transforms.add
def convert_dependencies(config, tasks):
    """
    Convert dependencies into soft-dependencies

    This means that taskcluster won't schedule the dependencies if only this
    task depends on them.
    """
    for task in tasks:
        task.setdefault("soft-dependencies", [])
        task["soft-dependencies"] += [
            dep_task.label
            for dep_task in config.kind_dependencies_tasks
        ]
        yield task

@transforms.add
def add_alert_routes(config, tasks):
    """
    Add routes to alert channels when this task fails.
    """
    for task in tasks:
        task.setdefault('routes', [])
        alerts = task.pop("alerts", {})
        for name, value in alerts.items():
            if name not in ("slack-channel", "email", "pulse", "matrix-room"):
                raise KeyError("Unknown alert type: {}".format(name))
            task['routes'].append("notify.{}.{}.on-failed".format(name, value))
        yield task

