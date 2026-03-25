import codecs

path = r'c:\Users\ADMIN\Desktop\remmy-drips\Remitwise-Contracts\savings_goals\src\test.rs'
with codecs.open(path, 'r', 'utf-8') as f:
    c = f.read()

reps = [
    ("client.get_goal(&id).locked", "client.get_goal(&id).unwrap().locked"),
    ("client.get_goal(&goal_id).current_amount", "client.get_goal(&goal_id).unwrap().current_amount"),
    ("let goal = client.get_goal(&id);", "let goal = client.get_goal(&id).unwrap();"),
    ("let goal1 = client.get_goal(&id1);", "let goal1 = client.get_goal(&id1).unwrap();"),
    ("let goal2 = client.get_goal(&id2);", "let goal2 = client.get_goal(&id2).unwrap();"),
    ("let g1 = client.get_goal(&id1);", "let g1 = client.get_goal(&id1).unwrap();"),
    ("let g2 = client.get_goal(&id2);", "let g2 = client.get_goal(&id2).unwrap();"),
    ("let goal = client.get_goal(&goal_id);", "let goal = client.get_goal(&goal_id).unwrap();"),
    ("let schedule = client.get_savings_schedule(&schedule_id);", "let schedule = client.get_savings_schedule(&schedule_id).unwrap();"),
    ("executed.get(0)", "executed.get(0).unwrap()"),
    ("Symbol::try_from_val(&env, &topics.get(0).unwrap());", "Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();"),
    ("GoalCreatedEvent::try_from_val(&env, &event.2);", "GoalCreatedEvent::try_from_val(&env, &event.2).unwrap();"),
    ("SavingsEvent::try_from_val(&env, &topics.get(1).unwrap());", "SavingsEvent::try_from_val(&env, &topics.get(1).unwrap()).unwrap();"),
    ("FundsAddedEvent::try_from_val(&env, &event.2);", "FundsAddedEvent::try_from_val(&env, &event.2).unwrap();"),
    ("GoalCompletedEvent::try_from_val(&env, &event.2);", "GoalCompletedEvent::try_from_val(&env, &event.2).unwrap();"),
    ("let last_id = first.items.get(1).id;", "let last_id = first.items.get(1).unwrap().id;"),
]

for old, new in reps:
    c = c.replace(old, new)

# To avoid executed.get(0).unwrap().unwrap(), let's replace back if happened
c = c.replace("executed.get(0).unwrap().unwrap()", "executed.get(0).unwrap()")

with codecs.open(path, 'w', 'utf-8') as f:
    f.write(c)

print("Restored necessary unwraps!")
