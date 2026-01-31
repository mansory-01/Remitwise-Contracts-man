use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

#[test]
fn test_create_goal_unique_ids() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();

    let name1 = String::from_str(&env, "Goal 1");
    let name2 = String::from_str(&env, "Goal 2");

    // Tell the environment to auto-approve the 'user' signature
    env.mock_all_auths();

    let id1 = client.create_goal(&user, &name1, &1000, &1735689600);
    let id2 = client.create_goal(&user, &name2, &2000, &1735689600);

    assert_ne!(id1, id2);
}

#[test]
fn test_add_to_goal_increments() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();

    env.mock_all_auths();
    let id = client.create_goal(&user, &String::from_str(&env, "Save"), &1000, &2000000000);

    let new_balance = client.add_to_goal(&user, &id, &500);
    assert_eq!(new_balance, 500);
}

#[test]
#[should_panic] // It will panic because the goal doesn't exist
fn test_add_to_non_existent_goal() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    client.add_to_goal(&user, &99, &500);
}

#[test]
fn test_get_goal_retrieval() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let name = String::from_str(&env, "Car");
    let id = client.create_goal(&user, &name, &5000, &2000000000);

    let goal = client.get_goal(&id).unwrap();
    assert_eq!(goal.name, name);
}

#[test]
fn test_get_all_goals() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    client.create_goal(&user, &String::from_str(&env, "A"), &100, &2000000000);
    client.create_goal(&user, &String::from_str(&env, "B"), &200, &2000000000);

    let all_goals = client.get_all_goals(&user);
    assert_eq!(all_goals.len(), 2);
}

#[test]
fn test_is_goal_completed() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();

    // 1. Create a goal with a target of 1000
    let target = 1000;
    let name = String::from_str(&env, "Trip");
    let id = client.create_goal(&user, &name, &target, &2000000000);

    // 2. It should NOT be completed initially (balance is 0)
    assert!(
        !client.is_goal_completed(&id),
        "Goal should not be complete at start"
    );

    // 3. Add exactly the target amount
    client.add_to_goal(&user, &id, &target);

    // 4. Verify the balance actually updated in storage
    let goal = client.get_goal(&id).unwrap();
    assert_eq!(
        goal.current_amount, target,
        "The amount was not saved correctly"
    );

    // 5. This will now pass once you fix the .instance() vs .persistent() mismatch in lib.rs
    assert!(
        client.is_goal_completed(&id),
        "Goal should be completed when current == target"
    );

    // 6. Bonus: Check that it stays completed if we go over the target
    client.add_to_goal(&user, &id, &1);
    assert!(
        client.is_goal_completed(&id),
        "Goal should stay completed if overfunded"
    );
}

#[test]
fn test_edge_cases_large_amounts() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(
        &user,
        &String::from_str(&env, "Max"),
        &i128::MAX,
        &2000000000,
    );

    client.add_to_goal(&user, &id, &(i128::MAX - 100));
    let goal = client.get_goal(&id).unwrap();
    assert_eq!(goal.current_amount, i128::MAX - 100);
}

#[test]
#[should_panic]
fn test_zero_amount_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    client.create_goal(&user, &String::from_str(&env, "Fail"), &0, &2000000000);
}

#[test]
fn test_multiple_goals_management() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id1 = client.create_goal(&user, &String::from_str(&env, "G1"), &1000, &2000000000);
    let id2 = client.create_goal(&user, &String::from_str(&env, "G2"), &2000, &2000000000);

    client.add_to_goal(&user, &id1, &500);
    client.add_to_goal(&user, &id2, &1500);

    let g1 = client.get_goal(&id1).unwrap();
    let g2 = client.get_goal(&id2).unwrap();

    assert_eq!(g1.current_amount, 500);
    assert_eq!(g2.current_amount, 1500);
}

#[test]
fn test_withdraw_from_goal() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(&user, &String::from_str(&env, "W"), &1000, &2000000000);

    // Unlock first (created locked)
    client.unlock_goal(&user, &id);

    client.add_to_goal(&user, &id, &500);

    let new_balance = client.withdraw_from_goal(&user, &id, &200);
    assert_eq!(new_balance, 300);

    let goal = client.get_goal(&id).unwrap();
    assert_eq!(goal.current_amount, 300);
}

#[test]
#[should_panic(expected = "Insufficient balance")]
fn test_withdraw_too_much() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(&user, &String::from_str(&env, "W"), &1000, &2000000000);

    client.unlock_goal(&user, &id);
    client.add_to_goal(&user, &id, &100);

    client.withdraw_from_goal(&user, &id, &200);
}

#[test]
#[should_panic(expected = "Cannot withdraw from a locked goal")]
fn test_withdraw_locked() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(&user, &String::from_str(&env, "L"), &1000, &2000000000);

    // Goal is locked by default
    client.add_to_goal(&user, &id, &500);
    client.withdraw_from_goal(&user, &id, &100);
}

#[test]
#[should_panic(expected = "Only the goal owner can withdraw funds")]
fn test_withdraw_unauthorized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let other = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(&user, &String::from_str(&env, "Auth"), &1000, &2000000000);

    client.unlock_goal(&user, &id);
    client.add_to_goal(&user, &id, &500);

    client.withdraw_from_goal(&other, &id, &100);
}

#[test]
fn test_lock_unlock_goal() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(&user, &String::from_str(&env, "Lock"), &1000, &2000000000);

    let goal = client.get_goal(&id).unwrap();
    assert!(goal.locked);

    client.unlock_goal(&user, &id);
    let goal = client.get_goal(&id).unwrap();
    assert!(!goal.locked);

    client.lock_goal(&user, &id);
    let goal = client.get_goal(&id).unwrap();
    assert!(goal.locked);
}

#[test]
fn test_full_withdrawal() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(&user, &String::from_str(&env, "W"), &1000, &2000000000);

    client.unlock_goal(&user, &id);
    client.add_to_goal(&user, &id, &500);

    // Withdraw everything
    let new_balance = client.withdraw_from_goal(&user, &id, &500);
    assert_eq!(new_balance, 0);

    let goal = client.get_goal(&id).unwrap();
    assert_eq!(goal.current_amount, 0);
    assert!(!client.is_goal_completed(&id));
}

#[test]
fn test_exact_goal_completion() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();
    let id = client.create_goal(&user, &String::from_str(&env, "Exact"), &1000, &2000000000);

    // Add 500 twice
    client.add_to_goal(&user, &id, &500);
    assert!(!client.is_goal_completed(&id));

    client.add_to_goal(&user, &id, &500);
    assert!(client.is_goal_completed(&id));

    let goal = client.get_goal(&id).unwrap();
    assert_eq!(goal.current_amount, 1000);
}

// ============================================
// Storage Optimization and Archival Tests
// ============================================

#[test]
fn test_archive_completed_goals() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();

    // Create goals with past target dates
    let id1 = client.create_goal(&user, &String::from_str(&env, "Goal1"), &1000, &1000000);
    let id2 = client.create_goal(&user, &String::from_str(&env, "Goal2"), &500, &1000000);
    // Create a goal with future target date
    let id3 = client.create_goal(&user, &String::from_str(&env, "Goal3"), &2000, &3000000000);

    // Complete goals 1 and 2
    client.add_to_goal(&user, &id1, &1000);
    client.add_to_goal(&user, &id2, &500);
    client.add_to_goal(&user, &id3, &1000); // Only partially funded

    // Archive completed goals with target date before timestamp 2000000
    let archived_count = client.archive_completed_goals(&user, &2000000);
    assert_eq!(archived_count, 2);

    // Verify active goals only has the incomplete one
    let active = client.get_all_goals(&user);
    assert_eq!(active.len(), 1);
    assert_eq!(active.get(0).unwrap().id, id3);

    // Verify archived goals
    let archived = client.get_archived_goals(&user);
    assert_eq!(archived.len(), 2);
}

#[test]
fn test_archive_empty_when_no_completed_goals() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();

    // Create incomplete goals
    client.create_goal(&user, &String::from_str(&env, "Goal1"), &1000, &1000000);
    client.create_goal(&user, &String::from_str(&env, "Goal2"), &500, &1000000);

    // Try to archive - should archive nothing
    let archived_count = client.archive_completed_goals(&user, &2000000);
    assert_eq!(archived_count, 0);

    // Verify all goals still active
    let active = client.get_all_goals(&user);
    assert_eq!(active.len(), 2);

    let archived = client.get_archived_goals(&user);
    assert_eq!(archived.len(), 0);
}

#[test]
fn test_get_archived_goal() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();

    let id = client.create_goal(&user, &String::from_str(&env, "Archive"), &1000, &1000000);
    client.add_to_goal(&user, &id, &1000);

    client.archive_completed_goals(&user, &2000000);

    // Get specific archived goal
    let archived_goal = client.get_archived_goal(&id);
    assert!(archived_goal.is_some());
    let goal = archived_goal.unwrap();
    assert_eq!(goal.id, id);
    assert_eq!(goal.final_amount, 1000);
}

#[test]
fn test_restore_goal() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();

    let id = client.create_goal(&user, &String::from_str(&env, "Restore"), &1000, &1000000);
    client.add_to_goal(&user, &id, &1500);

    // Archive the goal
    client.archive_completed_goals(&user, &2000000);
    assert!(client.get_goal(&id).is_none());
    assert!(client.get_archived_goal(&id).is_some());

    // Restore the goal
    let restored = client.restore_goal(&user, &id);
    assert!(restored);

    // Verify goal is back in active storage
    let goal = client.get_goal(&id);
    assert!(goal.is_some());
    let goal = goal.unwrap();
    assert_eq!(goal.current_amount, 1500);

    // Verify goal is no longer in archive
    assert!(client.get_archived_goal(&id).is_none());
}

#[test]
#[should_panic(expected = "Archived goal not found")]
fn test_restore_nonexistent_goal() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();

    client.restore_goal(&user, &999);
}

#[test]
#[should_panic(expected = "Only the goal owner can restore this goal")]
fn test_restore_unauthorized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    let other = Address::generate(&env);

    client.init();
    env.mock_all_auths();

    let id = client.create_goal(&user, &String::from_str(&env, "Auth"), &1000, &1000000);
    client.add_to_goal(&user, &id, &1000);
    client.archive_completed_goals(&user, &2000000);

    // Try to restore as different user
    client.restore_goal(&other, &id);
}

#[test]
fn test_cleanup_old_archives() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();

    // Create and complete multiple goals
    let id1 = client.create_goal(&user, &String::from_str(&env, "Old1"), &100, &1000);
    let id2 = client.create_goal(&user, &String::from_str(&env, "Old2"), &200, &1000);
    client.add_to_goal(&user, &id1, &100);
    client.add_to_goal(&user, &id2, &200);

    // Archive all
    client.archive_completed_goals(&user, &2000);

    // Verify 2 archived
    assert_eq!(client.get_archived_goals(&user).len(), 2);

    // Cleanup archives older than far future timestamp (should delete all since archived_at is current time which is 0 in test)
    let deleted = client.cleanup_old_archives(&user, &1000000);
    assert_eq!(deleted, 2);

    // Verify archives are gone
    assert_eq!(client.get_archived_goals(&user).len(), 0);
}

#[test]
fn test_storage_stats() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    client.init();
    env.mock_all_auths();

    // Initial stats
    let stats = client.get_storage_stats();
    assert_eq!(stats.active_goals, 0);
    assert_eq!(stats.archived_goals, 0);

    // Create goals and add funds
    let id1 = client.create_goal(&user, &String::from_str(&env, "G1"), &1000, &1000);
    let id2 = client.create_goal(&user, &String::from_str(&env, "G2"), &500, &1000);
    client.add_to_goal(&user, &id1, &1000);
    client.add_to_goal(&user, &id2, &500);

    // Archive one
    client.archive_completed_goals(&user, &2000);

    // Check updated stats
    let stats = client.get_storage_stats();
    assert_eq!(stats.active_goals, 0);
    assert_eq!(stats.archived_goals, 2);
    assert_eq!(stats.total_archived_amount, 1500);
}

#[test]
fn test_archive_preserves_owner_separation() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    client.init();
    env.mock_all_auths();

    // Create goals for different users
    let id1 = client.create_goal(&user1, &String::from_str(&env, "U1G1"), &1000, &1000);
    let id2 = client.create_goal(&user2, &String::from_str(&env, "U2G1"), &500, &1000);
    client.add_to_goal(&user1, &id1, &1000);
    client.add_to_goal(&user2, &id2, &500);

    // Archive all
    client.archive_completed_goals(&user1, &2000);

    // User1 should only see their archived goals
    let user1_archived = client.get_archived_goals(&user1);
    assert_eq!(user1_archived.len(), 1);
    assert_eq!(user1_archived.get(0).unwrap().owner, user1);

    // User2 should only see their archived goals
    let user2_archived = client.get_archived_goals(&user2);
    assert_eq!(user2_archived.len(), 1);
    assert_eq!(user2_archived.get(0).unwrap().owner, user2);
}
